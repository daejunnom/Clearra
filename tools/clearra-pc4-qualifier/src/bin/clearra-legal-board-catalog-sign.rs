//! Sign only five byte-identical, source-chain-verified PC4 legal-board bundles.
//!
//! The independent full-family receipts are bound by their candidate payload
//! hashes. This one-shot tool rechecks every materialized F/R/L source chain
//! before reading its signing seed from stdin. The seed is never printed or
//! written; publication and application release remain separate gates.

use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use clearra_accelerator_activation::{
    verify_accelerator_envelope, PinnedPublicKey, StaticPublicKeyring, ASSET_STATEMENT_SCHEMA,
    SIGNATURE_ALGORITHM, SIGNATURE_DOMAIN, SIGNED_ASSET_ENVELOPE_SCHEMA,
};
use clearra_core_executor::{
    accelerator_profile_name, built_in_legal_board_binding, ExactLegalBoard, LegalBoardExpectation,
    EXACT_LEGAL_BOARD_COMPLETENESS_SCOPE,
};
use clearra_pc4_qualifier::{
    legal_board_source_chain_identity, validate_legal_board_candidate_catalog,
    verify_legal_board_candidate_source_chain, LegalBoardGenerationOptions,
};
use clearra_rules::kicks::KickTableProfileId;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use sha2::{Digest, Sha256};

const REPOSITORY: &str = "daejunnom/Clearra";
const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 320 * 1024 * 1024;
const MAX_CATALOG_BYTES: usize = 512 * 1024;
const ACTIVE_SHARED_LIMIT: usize = 128 * 1024 * 1024;
const PROFILES: [(KickTableProfileId, &str, &str); 5] = [
    (
        KickTableProfileId::Srs90,
        "srs",
        "0dee2f88b01c8f44df8c2e5f3d5b7f6b76feb772b4e2fb7b7daab4165b155160",
    ),
    (
        KickTableProfileId::SrsPlus,
        "srs-plus",
        "e830202b61034a3dcdf7c0e03cb3b94a5bbd06a6e0b651776a5a14d6b6a6e7d2",
    ),
    (
        KickTableProfileId::SrsX,
        "srs-x",
        "d6ed2ef22dfe60cf98708360e98743c003aaff53f4c559b8bbfbb44ff0b371e6",
    ),
    (
        KickTableProfileId::Jstris180,
        "jstris-180",
        "7d20636d3f6e26415dcac7afda161f34916848e1cbc165ca030030eda087d30a",
    ),
    (
        KickTableProfileId::NoKick,
        "no-kick",
        "956b55b8797b37fe3470ee17a497e664b7151f559c8db280ed510e179b85b669",
    ),
];

struct VerifiedInput {
    profile: KickTableProfileId,
    name: &'static str,
    bundle_bytes: usize,
    shared_bytes: usize,
    bundle_identity: [u8; 32],
    catalog_identity: [u8; 32],
    chain_identity: [u8; 32],
    generation_identity: [u8; 32],
    rule_identity: [u8; 32],
    layer_counts: [u64; 11],
    layer_payload_identities: [[u8; 32]; 11],
}

fn main() {
    if let Err(error) = run() {
        eprintln!("legal-board catalog signing refused: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mut candidate_root = None;
    let mut output_catalog = None;
    let mut revision = None;
    let mut release_tag = None;
    while let Some(option) = args.next() {
        let value = args.next().ok_or("missing signing option value")?;
        match option.as_str() {
            "--candidate-root" if candidate_root.is_none() => {
                candidate_root = Some(PathBuf::from(value))
            }
            "--output-catalog" if output_catalog.is_none() => {
                output_catalog = Some(PathBuf::from(value))
            }
            "--revision" if revision.is_none() => revision = Some(value),
            "--release-tag" if release_tag.is_none() => release_tag = Some(value),
            _ => return Err("unknown or repeated signing option".into()),
        }
    }
    let candidate_root = candidate_root.ok_or("missing candidate root")?;
    let output_catalog = output_catalog.ok_or("missing output catalog")?;
    let revision: String = revision.ok_or("missing exact source revision")?;
    let release_tag: String = release_tag.ok_or("missing immutable release tag")?;
    if !candidate_root.is_absolute()
        || !output_catalog.is_absolute()
        || !real_directory(&candidate_root)
        || !output_catalog.parent().is_some_and(real_directory)
        || revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || release_tag.is_empty()
        || !release_tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("signing path, revision or tag is outside its contract".into());
    }

    // The source verifier reconstructs each immutable bundle from every
    // materialized forward/reverse/legal layer. The known full-family hash
    // then binds this qualification to the already measured candidate.
    let mut verified = Vec::with_capacity(PROFILES.len());
    let mut total_bytes = 0_usize;
    for (profile, name, expected_hash) in PROFILES {
        let layers = candidate_root.join(name);
        let bundle = layers.join(format!("legal-board-{name}-v2.cllb"));
        let catalog = layers.join(format!("legal-board-{name}-v2.catalog.json"));
        verify_legal_board_candidate_source_chain(&LegalBoardGenerationOptions {
            profile,
            layers,
            bundle: bundle.clone(),
            catalog: catalog.clone(),
            workers: 1,
            max_new_steps: 1,
        })?;
        let bundle_bytes = read_bounded_regular(&bundle, MAX_BUNDLE_BYTES)?;
        let catalog_bytes = read_bounded_regular(&catalog, MAX_CATALOG_BYTES)?;
        let actual_hash: [u8; 32] = Sha256::digest(&bundle_bytes).into();
        if hex(actual_hash) != expected_hash {
            return Err(format!(
                "{name} source chain differs from the full-family candidate"
            ));
        }
        let summary = validate_legal_board_candidate_catalog(
            profile,
            bundle
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("bundle name is not UTF-8")?,
            Arc::from(bundle_bytes.as_slice()),
            &catalog_bytes,
        )
        .map_err(str::to_owned)?;
        let binding = built_in_legal_board_binding(profile)
            .map_err(|_| "legal-board rule binding unavailable")?;
        let board = ExactLegalBoard::load(
            Arc::from(bundle_bytes.as_slice()),
            LegalBoardExpectation {
                binding,
                generation_identity: Some(summary.generation_identity),
            },
        )
        .map_err(|_| "source-bound legal-board payload invalid")?;
        let shared_bytes = summary
            .bundle_bytes
            .saturating_add(summary.sparse_index_bytes);
        if shared_bytes > ACTIVE_SHARED_LIMIT {
            return Err(format!(
                "{name} exceeds the active-session shared memory bound"
            ));
        }
        total_bytes = total_bytes.saturating_add(summary.bundle_bytes);
        let mut layer_payload_identities = [[0_u8; 32]; 11];
        for (layer, destination) in layer_payload_identities.iter_mut().enumerate() {
            *destination = board
                .layer_payload_digest(layer)
                .ok_or("legal-board layer directory incomplete")?;
        }
        let catalog_identity: [u8; 32] = Sha256::digest(&catalog_bytes).into();
        let chain_identity =
            legal_board_source_chain_identity(profile, catalog_identity, actual_hash)?;
        verified.push(VerifiedInput {
            profile,
            name,
            bundle_bytes: summary.bundle_bytes,
            shared_bytes,
            bundle_identity: actual_hash,
            catalog_identity,
            chain_identity,
            generation_identity: summary.generation_identity,
            rule_identity: binding.rule_identity,
            layer_counts: summary.layer_counts,
            layer_payload_identities,
        });
    }
    if total_bytes > MAX_TOTAL_BYTES {
        return Err("five legal-board bundles exceed the aggregate 320MiB bound".into());
    }

    // No seed is requested until all five complete chains, payloads and
    // known full-family candidate identities have passed qualification.
    let mut seed_text = String::new();
    std::io::stdin()
        .take(67)
        .read_to_string(&mut seed_text)
        .map_err(|_| "Ed25519 seed input unavailable")?;
    let seed = decode_seed(seed_text.trim_end_matches(['\r', '\n']))
        .ok_or("Ed25519 seed must be 32 bytes of lowercase hex")?;
    let signing = SigningKey::from_bytes(&seed);
    let public_key = signing.verifying_key().to_bytes();
    let key_id = format!("ed25519-raw-sha256:{}", hex(Sha256::digest(public_key)));
    let pinned_key_id: &'static str = Box::leak(key_id.clone().into_boxed_str());
    let keyring = [PinnedPublicKey {
        key_id: pinned_key_id,
        public_key,
    }];
    let mut profiles = Vec::with_capacity(PROFILES.len());
    for input in verified {
        if accelerator_profile_name(input.profile).map_err(|_| "unknown legal-board profile")?
            != input.name
        {
            return Err("source profile changed during signing".into());
        }
        let mut qualification = Sha256::new();
        qualification.update(b"clearra.v081.legal-board-product-qualification.v1\0");
        qualification.update(input.name.as_bytes());
        qualification.update(input.bundle_identity);
        qualification.update(input.catalog_identity);
        let qualification_identity: [u8; 32] = qualification.finalize().into();
        let url = format!(
            "https://github.com/{REPOSITORY}/releases/download/{release_tag}/legal-board-{}.cllb",
            input.name
        );
        let statement_json = serde_json::to_string(&json!({
            "algorithm": SIGNATURE_ALGORITHM,
            "asset_url": url,
            "completeness_scope": EXACT_LEGAL_BOARD_COMPLETENESS_SCOPE,
            "generation_identity": hex(input.generation_identity),
            "key_id": key_id,
            "payload_bytes": input.bundle_bytes.to_string(),
            "payload_identity": hex(input.bundle_identity),
            "product": "exact-legal-board",
            "profile": input.name,
            "qualification_identity": hex(qualification_identity),
            "repository": REPOSITORY,
            "revision": revision,
            "rule_identity": hex(input.rule_identity),
            "schema": ASSET_STATEMENT_SCHEMA,
        }))
        .map_err(|_| "statement encoding failed")?;
        let mut signed = SIGNATURE_DOMAIN.to_vec();
        signed.extend_from_slice(statement_json.as_bytes());
        let signature = signing.sign(&signed);
        let envelope = serde_json::to_string(&json!({
            "schema": SIGNED_ASSET_ENVELOPE_SCHEMA,
            "signature_hex": hex(signature.to_bytes()),
            "statement_json": statement_json,
        }))
        .map_err(|_| "envelope encoding failed")?;
        let authority = verify_accelerator_envelope(&envelope, StaticPublicKeyring::new(&keyring))
            .map_err(|_| "newly signed envelope did not verify")?;
        if authority.generation_identity() != input.generation_identity {
            return Err("signed generation differs from its verified source chain".into());
        }
        profiles.push(json!({
            "activation_envelope_json": envelope,
            "metadata": {
                "active_session_shared_bytes": input.shared_bytes.to_string(),
                "chain_identity": hex(input.chain_identity),
                "generation_identity": hex(input.generation_identity),
                "layer_counts": input.layer_counts.map(|count| count.to_string()),
                "layer_payload_identities": input.layer_payload_identities.map(hex),
                "payload_bytes": input.bundle_bytes.to_string(),
                "payload_identity": hex(input.bundle_identity),
                "qualification_identity": hex(qualification_identity),
                "rule_identity": hex(input.rule_identity),
                "url": url,
            },
            "profile": input.name,
            "status": "qualified",
        }));
    }
    let catalog = serde_json::to_string_pretty(&json!({
        "product": "exact-legal-board",
        "profiles": profiles,
        "schema": "clearra.legal-board.product-catalog.v1",
    }))
    .map_err(|_| "product catalog encoding failed")?
        + "\n";
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_catalog)
        .map_err(|_| "product catalog output already exists or is unsafe")?;
    if output
        .write_all(catalog.as_bytes())
        .and_then(|_| output.sync_all())
        .is_err()
    {
        drop(output);
        let _ = fs::remove_file(&output_catalog);
        return Err("product catalog write did not complete".into());
    }
    println!("legal_board_catalog=source_bound_signed profiles=5 aggregate_bytes={total_bytes}");
    println!("public_key_hex={}", hex(public_key));
    println!("key_id={key_id}");
    Ok(())
}

fn real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
}

fn read_bounded_regular(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "candidate input is missing")?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > maximum as u64
    {
        return Err("candidate input is not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(metadata.len() as usize)
        .map_err(|_| "candidate input allocation failed")?;
    fs::File::open(path)
        .map_err(|_| "candidate input cannot be opened")?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "candidate input cannot be read")?;
    if bytes.is_empty() || bytes.len() > maximum {
        return Err("candidate input changed size during read".into());
    }
    Ok(bytes)
}

fn decode_seed(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut seed = [0_u8; 32];
    for (index, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(seed)
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
