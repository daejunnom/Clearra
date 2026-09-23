use clearra_pc4_qualifier::{
    generate_legal_board, validate_legal_board_candidate_catalog,
    verify_legal_board_candidate_source_chain, LegalBoardGenerationOptions,
};
use clearra_rules::kicks::KickTableProfileId;
use std::{collections::BTreeMap, fs, io::Read, path::PathBuf, sync::Arc};

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
            layers,
            bundle,
            catalog,
            workers: 1,
            max_new_steps: 1,
        })?;
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
            "profile" | "layers" | "workers" | "max-new-steps" | "bundle" | "catalog"
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
    Ok((action, options))
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
