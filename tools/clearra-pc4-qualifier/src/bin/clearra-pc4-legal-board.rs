use clearra_pc4_qualifier::{generate_legal_board, LegalBoardGenerationOptions};
use clearra_rules::kicks::KickTableProfileId;
use std::{collections::BTreeMap, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("pc4_legal_board_error={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = parse_options()?;
    let profile = KickTableProfileId::parse(required(&options, "profile")?)
        .ok_or("legal-board profile is not a known kick-table profile")?;
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

fn parse_options() -> Result<BTreeMap<String, String>, String> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("legal-board-run") {
        return Err("expected legal-board-run".to_owned());
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
    Ok(options)
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
