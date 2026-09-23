use crate::domain;
use clearra_rules::kicks::KickTableProfileId;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

// The exact meet may occur at any interior layer. Five keeps the unrestricted
// forward frontier one layer smaller; the additional reverse layer remains
// exact and is checked by the same proof chain before publication.
const MEET_LAYER: u8 = 5;

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
    for input_layer in 0_u8..MEET_LAYER {
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

    // Exact bidirectional meet: F_m is complete from the empty board and
    // R_(m+1) is complete to the full board. An F_m state with an exact ILC
    // edge into R_(m+1) is precisely L_m. This avoids materializing the
    // unrestricted forward suffix while preserving the same L_0..L_10.
    let reverse = domain::DomainDirection::Reverse;
    let terminal = domain::seed(
        binding,
        reverse,
        &layers.join("reverse-filter-layer-10.bin"),
    )?;
    println!(
        "pc4_domain_seed={} direction=reverse-filter layer={} fields={} identity={}",
        terminal.disposition, terminal.layer, terminal.field_count, terminal.file_identity
    );
    if terminal.disposition == "created" {
        created += 1;
        if created == options.max_new_steps {
            return incomplete("reverse-filter", created, options.max_new_steps);
        }
    }

    for source_layer in ((MEET_LAYER + 1)..10).rev() {
        let input_layer = source_layer + 1;
        let report = domain::step(
            binding,
            reverse,
            &layers.join(format!("reverse-filter-layer-{input_layer:02}.bin")),
            None,
            &layers.join(format!("reverse-filter-layer-{source_layer:02}.bin")),
            options.workers,
        )?;
        println!(
            "pc4_domain_step={} direction=reverse-filter input_layer={} output_layer={} input_fields={} output_fields={} workers={} identity={}",
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
                return incomplete("reverse-filter", created, options.max_new_steps);
            }
        }
    }

    let report = domain::legal_predecessor_step(
        binding,
        &layers.join(format!("forward-reachable-layer-{MEET_LAYER:02}.bin")),
        &layers.join(format!("reverse-filter-layer-{:02}.bin", MEET_LAYER + 1)),
        &layers.join(format!("legal-layer-{MEET_LAYER:02}.bin")),
        options.workers,
    )?;
    println!(
        "pc4_domain_step={} direction=legal-meet input_layer={} output_layer={} input_fields={} output_fields={} workers={} identity={}",
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
            return incomplete("legal-meet", created, options.max_new_steps);
        }
    }

    for source_layer in (0_u8..MEET_LAYER).rev() {
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
            if created == options.max_new_steps {
                return incomplete("legal-backtrace", created, options.max_new_steps);
            }
        }
    }

    for output_layer in (MEET_LAYER + 1)..=10 {
        let input_layer = output_layer - 1;
        let report = domain::step(
            binding,
            forward,
            &layers.join(format!("legal-layer-{input_layer:02}.bin")),
            Some(&layers.join(format!("reverse-filter-layer-{output_layer:02}.bin"))),
            &layers.join(format!("legal-layer-{output_layer:02}.bin")),
            options.workers,
        )?;
        println!(
            "pc4_domain_step={} direction=legal-forward input_layer={} output_layer={} input_fields={} output_fields={} workers={} identity={}",
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
            if created == options.max_new_steps && output_layer != 10 {
                return incomplete("legal-forward", created, options.max_new_steps);
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
    let mut legal_summaries = Vec::with_capacity(11);
    let mut forward_identity = Vec::with_capacity(11);
    let mut reverse_identity = Vec::with_capacity(11);
    let mut legal_identity = Vec::with_capacity(11);
    let mut forward_digests = [None; 11];
    let mut reverse_digests = [None; 11];
    let mut legal_digests = [[0_u8; 32]; 11];
    let mut legal_input_digests = [[0_u8; 32]; 11];
    let mut legal_filter_digests = [[0_u8; 32]; 11];
    for layer in 0_u8..=10 {
        let legal_path = options.layers.join(format!("legal-layer-{layer:02}.bin"));
        let expected_derivation = if layer <= MEET_LAYER {
            domain::DomainDerivation::LegalPredecessorStep
        } else {
            domain::DomainDerivation::ForwardStep
        };
        let complete_path = if layer <= MEET_LAYER {
            options
                .layers
                .join(format!("forward-reachable-layer-{layer:02}.bin"))
        } else {
            options
                .layers
                .join(format!("reverse-filter-layer-{layer:02}.bin"))
        };
        let (legal, complete) =
            domain::verify_subset_files(&legal_path, &complete_path, binding, layer)?;
        if legal.derivation != expected_derivation {
            return Err("legal-board layer uses the wrong meet-side derivation".to_owned());
        }
        if layer <= MEET_LAYER {
            forward_digests[usize::from(layer)] = Some(complete.file_digest);
            forward_identity.push(Some(complete.file_identity));
            reverse_identity.push(None);
        } else {
            reverse_digests[usize::from(layer)] = Some(complete.file_digest);
            forward_identity.push(None);
            reverse_identity.push(Some(complete.file_identity));
        }
        legal_digests[usize::from(layer)] = legal.file_digest;
        legal_input_digests[usize::from(layer)] = legal.input_digest;
        legal_filter_digests[usize::from(layer)] = legal.filter_digest;
        legal_identity.push(legal.file_identity.clone());
        legal_summaries.push(legal);
    }
    for layer in 0..=10_usize {
        let expected_input = if layer <= usize::from(MEET_LAYER) {
            forward_digests[layer].ok_or("legal-board forward proof is incomplete")?
        } else {
            legal_digests[layer - 1]
        };
        let expected_filter = if layer < usize::from(MEET_LAYER) {
            legal_digests[layer + 1]
        } else {
            let reverse_layer = if layer == usize::from(MEET_LAYER) {
                layer + 1
            } else {
                layer
            };
            reverse_digests[reverse_layer].ok_or("legal-board reverse proof is incomplete")?
        };
        if legal_input_digests[layer] != expected_input
            || legal_filter_digests[layer] != expected_filter
        {
            return Err("legal-board layer proof chain is disconnected".to_owned());
        }
    }
    if legal_summaries[10].field_count != 1 {
        return Err("legal-board terminal is not the full four-line field".to_owned());
    }
    let encoded = clearra_core_executor::encode_exact_legal_board_intersection_streaming(
        binding.legal_board_binding(),
        |layer, emit| {
            let path = options.layers.join(format!("legal-layer-{layer:02}.bin"));
            domain::visit_verified_domain_fields(
                &path,
                binding,
                &legal_summaries[layer],
                &mut |field| emit(field).map_err(|error| error.code().to_owned()),
            )
        },
    )
    .map_err(|error| match error {
        clearra_core_executor::LegalBoardStreamEncodeError::Asset(asset) => asset.code().to_owned(),
        clearra_core_executor::LegalBoardStreamEncodeError::Source(source) => source,
    })?;
    let encoded: Arc<[u8]> = Arc::from(encoded);
    let generation: [u8; 32] = encoded[80..112]
        .try_into()
        .map_err(|_| "legal-board generation identity width mismatch")?;
    let loaded = clearra_core_executor::ExactLegalBoard::load(
        Arc::clone(&encoded),
        clearra_core_executor::LegalBoardExpectation {
            binding: binding.legal_board_binding(),
            generation_identity: Some(generation),
        },
    )
    .map_err(|error| error.code().to_owned())?;
    publish_immutable(&options.bundle, encoded.as_ref())?;

    let bundle_digest: [u8; 32] = Sha256::digest(encoded.as_ref()).into();
    let layer_entries = (0..=10)
        .map(|layer| {
            serde_json::json!({
                "layer": layer,
                "field_count": loaded.layer_count(layer).expect("all layers indexed"),
                "payload_sha256": format!("sha256:{}", hex(&loaded.layer_payload_digest(layer).expect("all layers indexed"))),
                "forward_domain_identity": forward_identity[layer],
                "reverse_domain_identity": reverse_identity[layer],
                "legal_layer_identity": legal_identity[layer],
            })
        })
        .collect::<Vec<_>>();
    let catalog = serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "clearra.legal-board.catalog.candidate.v2",
        "status": "candidate_unqualified",
        "construction": "bidirectional_exact_intersection_v1",
        "meet_layer": MEET_LAYER,
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
            "forward_prefix_complete": true,
            "reverse_suffix_complete": true,
            "inductive_legal_layers_complete": true,
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
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("existing legal-board output is a symlink or non-file".to_owned());
            }
            if metadata.len() != bytes.len() as u64 {
                return Err(
                    "refusing to replace a different immutable legal-board output".to_owned(),
                );
            }
            let mut existing = Vec::with_capacity(bytes.len());
            File::open(path)
                .map_err(|error| error.to_string())?
                .take((bytes.len() as u64).saturating_add(1))
                .read_to_end(&mut existing)
                .map_err(|error| error.to_string())?;
            if existing == bytes {
                return Ok(());
            }
            return Err("refusing to replace a different immutable legal-board output".to_owned());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
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
        fs::hard_link(&pending, path).map_err(|error| error.to_string())?;
        fs::remove_file(&pending).map_err(|error| error.to_string())?;
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

#[cfg(test)]
mod immutable_output_tests {
    use super::*;

    #[test]
    fn existing_output_is_idempotent_and_bounded_by_expected_length() {
        let root = std::env::temp_dir().join(format!(
            "clearra-legal-board-immutable-{}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let output = root.join("candidate.bin");
        publish_immutable(&output, b"first").unwrap();
        publish_immutable(&output, b"first").unwrap();
        assert!(publish_immutable(&output, b"other").is_err());
        OpenOptions::new()
            .write(true)
            .open(&output)
            .unwrap()
            .set_len(1024 * 1024)
            .unwrap();
        assert!(publish_immutable(&output, b"first").is_err());
        fs::remove_file(output).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dangling_link_is_never_treated_as_a_new_output() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "clearra-legal-board-dangling-{}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let output = root.join("candidate.bin");
        symlink(root.join("absent.bin"), &output).unwrap();
        assert!(publish_immutable(&output, b"new").is_err());
        fs::remove_file(output).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
