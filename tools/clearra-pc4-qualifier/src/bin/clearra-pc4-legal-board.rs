#[allow(dead_code)]
#[path = "../domain.rs"]
mod domain;

use clearra_rules::kicks::KickTableProfileId;
use std::{collections::BTreeMap, path::Path};

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
    let layers = Path::new(required(&options, "layers")?);
    if !layers.is_absolute() {
        return Err("--layers must be absolute".to_owned());
    }
    let metadata = std::fs::symlink_metadata(layers).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("--layers must be a real directory".to_owned());
    }
    let workers = numeric(&options, "workers")?;
    if !(1..=64).contains(&workers) {
        return Err("--workers outside 1..=64".to_owned());
    }
    let max_new_steps = numeric(&options, "max-new-steps")?;
    if !(1..=10).contains(&max_new_steps) {
        return Err("--max-new-steps outside 1..=10".to_owned());
    }

    let binding = domain::DomainBinding::legal_board(profile)?;
    println!(
        "pc4_legal_board_generation=begin profile={} binding={}",
        profile.as_str(),
        binding.identity_string()
    );
    run_reverse_layers(binding, layers, workers, max_new_steps)
}

fn run_reverse_layers(
    binding: domain::DomainBinding,
    layers: &Path,
    workers: usize,
    max_new_steps: usize,
) -> Result<(), String> {
    let direction = domain::DomainDirection::Reverse;
    let seed_path = layers.join("reverse-layer-10.bin");
    let seed = domain::seed(binding, direction, &seed_path)?;
    println!(
        "pc4_domain_seed={} direction=reverse layer={} fields={} identity={}",
        seed.disposition, seed.layer, seed.field_count, seed.file_identity
    );

    let mut created = 0_usize;
    for input_layer in (1_u8..=10).rev() {
        let output_layer = input_layer - 1;
        let input = layers.join(format!("reverse-layer-{input_layer:02}.bin"));
        let output = layers.join(format!("reverse-layer-{output_layer:02}.bin"));
        let report = domain::step(binding, direction, &input, None, &output, workers)?;
        println!(
            "pc4_domain_step={} direction=reverse input_layer={} output_layer={} input_fields={} output_fields={} candidate_pairs={} workers={} identity={}",
            report.disposition,
            report.input_layer,
            report.output_layer,
            report.input_field_count,
            report.output_field_count,
            report.candidate_pair_count,
            report.workers,
            report.file_identity
        );
        if report.disposition == "created" {
            created += 1;
            if created == max_new_steps {
                break;
            }
        }
    }
    println!(
        "pc4_domain_run=complete direction=reverse new_steps={created} max_new_steps={max_new_steps}"
    );
    Ok(())
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
        if !matches!(key, "profile" | "layers" | "workers" | "max-new-steps") {
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
