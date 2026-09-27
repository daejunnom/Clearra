//! One-shot, source-bound signing of the five v0.8.1 conditioned data assets.
//!
//! The Ed25519 seed arrives only on stdin and is never persisted or printed.
//! A caller may separately store that same seed in its credential service.
//! This binary does not upload assets or grant the application release gate.

use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use clearra_accelerator_activation::{
    verify_accelerator_envelope, PinnedPublicKey, StaticPublicKeyring, ASSET_STATEMENT_SCHEMA,
    SIGNATURE_ALGORITHM, SIGNATURE_DOMAIN, SIGNED_ASSET_ENVELOPE_SCHEMA,
};
use clearra_core_executor::built_in_local_relation_binding;
use clearra_pc4_qualifier::verify_v081_conditioned_product_candidate;
use clearra_rules::kicks::KickTableProfileId;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const PROFILES: [(KickTableProfileId, &str); 5] = [
    (KickTableProfileId::Srs90, "srs"),
    (KickTableProfileId::SrsPlus, "srs-plus"),
    (KickTableProfileId::SrsX, "srs-x"),
    (KickTableProfileId::Jstris180, "jstris-180"),
    (KickTableProfileId::NoKick, "no-kick"),
];
const REPOSITORY: &str = "daejunnom/Clearra";
const MAX_PACK_BYTES: usize = 16 * 1024 * 1024;
const MAX_TOTAL_PACK_BYTES: usize = 80 * 1024 * 1024;
const MAX_CATALOG_BYTES: usize = 512 * 1024;
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const COMPLETENESS_SCOPE: &str =
    "width10-height1to6-solver-sky-bottom8-56-contexts-entry-first-exit-boolean";

fn main() {
    if let Err(error) = run() {
        eprintln!("conditioned catalog signing refused: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mut candidate_dir = None;
    let mut source_dir = None;
    let mut output_catalog = None;
    let mut revision = None;
    let mut tag = None;
    while let Some(option) = args.next() {
        let value = args.next().ok_or("missing signing option value")?;
        match option.as_str() {
            "--candidate-dir" if candidate_dir.is_none() => {
                candidate_dir = Some(PathBuf::from(value))
            }
            "--source-dir" if source_dir.is_none() => source_dir = Some(PathBuf::from(value)),
            "--output-catalog" if output_catalog.is_none() => {
                output_catalog = Some(PathBuf::from(value))
            }
            "--revision" if revision.is_none() => revision = Some(value),
            "--release-tag" if tag.is_none() => tag = Some(value),
            _ => return Err("unknown or repeated signing option".into()),
        }
    }
    let candidate_dir = candidate_dir.ok_or("missing candidate directory")?;
    let source_dir = source_dir.ok_or("missing source directory")?;
    let output_catalog = output_catalog.ok_or("missing output catalog")?;
    let revision = revision.ok_or("missing exact source revision")?;
    let tag = tag.ok_or("missing immutable release tag")?;
    if !candidate_dir.is_absolute()
        || !source_dir.is_absolute()
        || !output_catalog.is_absolute()
        || !is_real_directory(&candidate_dir)
        || !is_real_directory(&source_dir)
        || !output_catalog.parent().is_some_and(is_real_directory)
        || revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || tag.is_empty()
        || !tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("signing path, revision or tag is outside its contract".into());
    }

    // Validate every data source before asking for the seed. An incomplete
    // profile never reaches the signing operation or writes a partial catalog.
    let mut inputs = Vec::with_capacity(PROFILES.len());
    let mut aggregate_bytes = 0_usize;
    for (profile, name) in PROFILES {
        let pack =
            read_bounded_regular(&candidate_dir.join(format!("{name}.cllr")), MAX_PACK_BYTES)?;
        let candidate_catalog = read_bounded_regular(
            &candidate_dir.join(format!("{name}.candidate.json")),
            MAX_CATALOG_BYTES,
        )?;
        let source = read_bounded_regular(
            &source_dir.join(format!("{name}.solver-cover.v1.json")),
            MAX_SOURCE_BYTES,
        )?;
        let verified =
            verify_v081_conditioned_product_candidate(profile, &pack, &candidate_catalog, &source)?;
        aggregate_bytes = aggregate_bytes.saturating_add(pack.len());
        let candidate: Value = serde_json::from_slice(&candidate_catalog)
            .map_err(|_| "candidate catalog JSON changed during signing")?;
        let query_set_identity = candidate["query_set_identity"]
            .as_str()
            .ok_or("verified candidate query identity is absent")?
            .to_owned();
        inputs.push((profile, name, pack, source, verified, query_set_identity));
    }
    if aggregate_bytes > MAX_TOTAL_PACK_BYTES {
        return Err("five conditioned packs exceed the aggregate 80MiB cap".into());
    }

    let mut seed_text = String::new();
    std::io::stdin()
        .take(67)
        .read_to_string(&mut seed_text)
        .map_err(|_| "Ed25519 seed input unavailable")?;
    let seed_text = seed_text.trim_end_matches(['\r', '\n']);
    let seed = decode_seed(seed_text).ok_or("Ed25519 seed must be 32 bytes of lowercase hex")?;
    let signing = SigningKey::from_bytes(&seed);
    let public_key = signing.verifying_key().to_bytes();
    let key_id = format!("ed25519-raw-sha256:{}", hex(Sha256::digest(public_key)));
    let pinned_key_id: &'static str = Box::leak(key_id.clone().into_boxed_str());
    let keyring = [PinnedPublicKey {
        key_id: pinned_key_id,
        public_key,
    }];
    let mut profiles = Vec::with_capacity(PROFILES.len());
    for (profile, name, pack, source, verified, query_set_identity) in inputs {
        let binding = built_in_local_relation_binding(profile)
            .map_err(|_| "verified profile rule binding disappeared")?;
        let payload_identity: [u8; 32] = Sha256::digest(&pack).into();
        let source_identity: [u8; 32] = Sha256::digest(&source).into();
        let mut qualification = Sha256::new();
        qualification.update(b"clearra.v081.conditioned-product-qualification.v1\0");
        qualification.update(name.as_bytes());
        qualification.update(payload_identity);
        qualification.update(source_identity);
        qualification.update((verified.supported_contexts as u64).to_le_bytes());
        qualification.update((verified.bounded_cover.candidate.record_count as u64).to_le_bytes());
        qualification.update(verified.bounded_cover.visited_proof_nodes.to_le_bytes());
        let qualification_identity: [u8; 32] = qualification.finalize().into();
        let asset_url = format!(
            "https://github.com/{REPOSITORY}/releases/download/{tag}/conditioned-{name}.cllr"
        );
        let statement_json = serde_json::to_string(&json!({
            "algorithm": SIGNATURE_ALGORITHM,
            "asset_url": asset_url,
            "completeness_scope": COMPLETENESS_SCOPE,
            "generation_identity": hex(verified.bounded_cover.candidate.generation_identity),
            "key_id": key_id,
            "payload_bytes": pack.len().to_string(),
            "payload_identity": hex(payload_identity),
            "product": "board-conditioned-reachability",
            "profile": name,
            "qualification_identity": hex(qualification_identity),
            "repository": REPOSITORY,
            "revision": revision,
            "rule_identity": hex(binding.rule_identity),
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
        if authority.generation_identity() != verified.bounded_cover.candidate.generation_identity {
            return Err("signed generation differs from independently verified candidate".into());
        }
        let active_session_shared_bytes = verified
            .bounded_cover
            .candidate
            .logical_resident_bytes
            .saturating_add(pack.len());
        profiles.push(json!({
            "activation_envelope_json": envelope,
            "metadata": {
                "active_session_shared_bytes": active_session_shared_bytes.to_string(),
                "generation_identity": hex(authority.generation_identity()),
                "payload_bytes": pack.len().to_string(),
                "payload_identity": hex(payload_identity),
                "qualification_identity": hex(qualification_identity),
                "query_set_identity": query_set_identity,
                "record_count": verified.bounded_cover.candidate.record_count.to_string(),
                "rule_identity": hex(binding.rule_identity),
                "url": asset_url,
            },
            "profile": name,
            "status": "qualified",
        }));
    }
    let catalog = serde_json::to_string_pretty(&json!({
        "product": "board-conditioned-reachability",
        "profiles": profiles,
        "schema": "clearra.conditioned-reachability.product-catalog.v1",
    }))
    .map_err(|_| "product catalog encoding failed")?
        + "\n";
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_catalog)
        .map_err(|_| "product catalog output already exists or is unsafe")?;
    if file
        .write_all(catalog.as_bytes())
        .and_then(|_| file.sync_all())
        .is_err()
    {
        let _ = fs::remove_file(&output_catalog);
        return Err("product catalog write did not complete".into());
    }
    println!(
        "conditioned_catalog=source_bound_signed profiles=5 aggregate_bytes={aggregate_bytes}"
    );
    println!("public_key_hex={}", hex(public_key));
    println!("key_id={key_id}");
    Ok(())
}

fn is_real_directory(path: &Path) -> bool {
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
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
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
