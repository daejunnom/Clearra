use crate::domain;
use clearra_rules::kicks::KickTableProfileId;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct LegalBoardGenerationOptions {
    pub profile: KickTableProfileId,
    pub layers: PathBuf,
    pub bundle: PathBuf,
    pub catalog: PathBuf,
    pub workers: usize,
    pub max_new_steps: usize,
}

pub fn generate_legal_board(options: &LegalBoardGenerationOptions) -> Result<(), String> {
    validate_options(options)?;
    let binding = domain::DomainBinding::legal_board(options.profile)?;
    println!(
        "pc4_legal_board_generation=begin profile={} binding={}",
        clearra_core_executor::accelerator_profile_name(options.profile)
            .map_err(|_| "unsupported legal-board profile")?,
        binding.identity_string()
    );
    run_exact_layers(binding, options)
}

fn validate_options(options: &LegalBoardGenerationOptions) -> Result<(), String> {
    if !options.layers.is_absolute()
        || !options.bundle.is_absolute()
        || !options.catalog.is_absolute()
    {
        return Err("legal-board paths must be absolute".to_owned());
    }
    let metadata = fs::symlink_metadata(&options.layers).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("legal-board layers must be a real directory".to_owned());
    }
    if !(1..=64).contains(&options.workers) {
        return Err("legal-board workers outside 1..=64".to_owned());
    }
    if !(1..=21).contains(&options.max_new_steps) {
        return Err("legal-board max-new-steps outside 1..=21".to_owned());
    }
    validate_output_path(&options.bundle)?;
    validate_output_path(&options.catalog)
}

fn run_exact_layers(
    binding: domain::DomainBinding,
    options: &LegalBoardGenerationOptions,
) -> Result<(), String> {
    let layers = options.layers.as_path();
    let forward = domain::DomainDirection::Forward;
    let seed = domain::seed(
        binding,
        forward,
        &layers.join("forward-reachable-layer-00.bin"),
    )?;
    println!(
        "pc4_domain_seed={} direction=forward-reachable layer={} fields={} identity={}",
        seed.disposition, seed.layer, seed.field_count, seed.file_identity
    );

    let mut created = 0_usize;
    for input_layer in 0_u8..10 {
        let output_layer = input_layer + 1;
        let report = domain::step(
            binding,
            forward,
            &layers.join(format!("forward-reachable-layer-{input_layer:02}.bin")),
            None,
            &layers.join(format!("forward-reachable-layer-{output_layer:02}.bin")),
            options.workers,
        )?;
        println!(
            "pc4_domain_step={} direction=forward-reachable input_layer={} output_layer={} input_fields={} output_fields={} workers={} identity={}",
            report.disposition,
            report.input_layer,
            report.output_layer,
            report.input_field_count,
            report.output_field_count,
            report.workers,
            report.file_identity
        );
        if report.disposition == "created" {
            created += 1;
            if created == options.max_new_steps {
                return incomplete("forward-reachable", created, options.max_new_steps);
            }
        }
    }

    let terminal = domain::legal_terminal_seed(
        binding,
        &layers.join("forward-reachable-layer-10.bin"),
        &layers.join("legal-layer-10.bin"),
    )?;
    println!(
        "pc4_domain_seed={} direction=legal-backtrace layer={} fields={} identity={}",
        terminal.disposition, terminal.layer, terminal.field_count, terminal.file_identity
    );
    if terminal.disposition == "created" {
        created += 1;
        if created == options.max_new_steps {
            return incomplete("legal-terminal", created, options.max_new_steps);
        }
    }

    for source_layer in (0_u8..10).rev() {
        let target_layer = source_layer + 1;
        let report = domain::legal_predecessor_step(
            binding,
            &layers.join(format!("forward-reachable-layer-{source_layer:02}.bin")),
            &layers.join(format!("legal-layer-{target_layer:02}.bin")),
            &layers.join(format!("legal-layer-{source_layer:02}.bin")),
            options.workers,
        )?;
        println!(
            "pc4_domain_step={} direction=legal-backtrace input_layer={} output_layer={} input_fields={} output_fields={} workers={} identity={}",
            report.disposition,
            report.input_layer,
            report.output_layer,
            report.input_field_count,
            report.output_field_count,
            report.workers,
            report.file_identity
        );
        if report.disposition == "created" {
            created += 1;
            if created == options.max_new_steps && source_layer != 0 {
                return incomplete("legal-backtrace", created, options.max_new_steps);
            }
        }
    }

    publish_exact_bundle(binding, options)?;
    println!(
        "pc4_legal_board_generation=complete new_steps={} max_new_steps={} bundle={} catalog={}",
        created,
        options.max_new_steps,
        options.bundle.display(),
        options.catalog.display()
    );
    Ok(())
}

fn incomplete(phase: &str, created: usize, limit: usize) -> Result<(), String> {
    println!(
        "pc4_legal_board_generation=incomplete phase={phase} new_steps={created} max_new_steps={limit}"
    );
    Ok(())
}

fn publish_exact_bundle(
    binding: domain::DomainBinding,
    options: &LegalBoardGenerationOptions,
) -> Result<(), String> {
    let mut fields: [Vec<u64>; 11] = std::array::from_fn(|_| Vec::new());
    let mut forward_identity = Vec::with_capacity(11);
    let mut legal_identity = Vec::with_capacity(11);
    for layer in 0_u8..=10 {
        let forward = domain::read(
            &options
                .layers
                .join(format!("forward-reachable-layer-{layer:02}.bin")),
            binding,
            Some(layer),
        )?;
        let legal = domain::read(
            &options.layers.join(format!("legal-layer-{layer:02}.bin")),
            binding,
            Some(layer),
        )?;
        if legal
            .fields
            .iter()
            .any(|field| forward.fields.binary_search(field).is_err())
        {
            return Err("legal-board layer is not a subset of its forward domain".to_owned());
        }
        fields[usize::from(layer)] = legal.fields;
        forward_identity.push(forward.file_identity);
        legal_identity.push(legal.file_identity);
    }
    let encoded = clearra_core_executor::encode_exact_legal_board_intersection(
        binding.legal_board_binding(),
        &fields,
    )
    .map_err(|error| error.code().to_owned())?;
    let generation: [u8; 32] = encoded[80..112]
        .try_into()
        .map_err(|_| "legal-board generation identity width mismatch")?;
    let loaded = clearra_core_executor::ExactLegalBoard::load(
        Arc::from(encoded.clone()),
        clearra_core_executor::LegalBoardExpectation {
            binding: binding.legal_board_binding(),
            generation_identity: Some(generation),
        },
    )
    .map_err(|error| error.code().to_owned())?;
    publish_immutable(&options.bundle, &encoded)?;

    let bundle_digest: [u8; 32] = Sha256::digest(&encoded).into();
    let layer_entries = (0..=10)
        .map(|layer| {
            serde_json::json!({
                "layer": layer,
                "field_count": loaded.layer_count(layer).expect("all layers indexed"),
                "payload_sha256": format!("sha256:{}", hex(&loaded.layer_payload_digest(layer).expect("all layers indexed"))),
                "forward_domain_identity": forward_identity[layer],
                "legal_backtrace_identity": legal_identity[layer],
            })
        })
        .collect::<Vec<_>>();
    let catalog = serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "clearra.legal-board.catalog.candidate.v1",
        "status": "candidate_unqualified",
        "profile": clearra_core_executor::accelerator_profile_name(binding.legal_board_binding().kick_profile)
            .map_err(|_| "unsupported legal-board profile")?,
        "rule_identity": binding.identity_string(),
        "generation_identity": format!("sha256:{}", hex(&generation)),
        "bundle": {
            "file": options.bundle.file_name().and_then(|value| value.to_str()).ok_or("bundle name must be UTF-8")?,
            "bytes": encoded.len(),
            "sha256": format!("sha256:{}", hex(&bundle_digest)),
            "url": serde_json::Value::Null,
        },
        "qualification": {
            "exact_intersection": true,
            "forward_complete": true,
            "backtrace_restricted_to_forward_domain": true,
            "signed": false,
            "release_authority": false,
        },
        "layers": layer_entries,
    }))
    .map_err(|error| error.to_string())?;
    publish_immutable(&options.catalog, &catalog)
}

fn validate_output_path(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or("legal-board output has no parent")?;
    let metadata = fs::symlink_metadata(parent).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("legal-board output parent must be a real directory".to_owned());
    }
    Ok(())
}

fn publish_immutable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("existing legal-board output is a symlink or non-file".to_owned());
        }
        if fs::read(path).map_err(|error| error.to_string())? == bytes {
            return Ok(());
        }
        return Err("refusing to replace a different immutable legal-board output".to_owned());
    }
    let parent = path.parent().ok_or("legal-board output has no parent")?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("legal-board output name must be UTF-8")?;
    let pending = parent.join(format!(".{name}.pending-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&pending, path).map_err(|error| error.to_string())?;
        Ok(())
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
