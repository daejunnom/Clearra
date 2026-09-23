//! Native persistent-store boundary for signed solver accelerator assets.
//!
//! The store never grants authority: it re-verifies the checked-in signed
//! catalog, structural payload and product-specific qualification every time a
//! process activates an installed generation. Publication uses an append-only,
//! fsynced activation journal so Windows never needs a non-atomic overwrite.

use clearra_accelerator_product_host::{
    embedded_catalog, CatalogProfileStatus, ProductCatalogKind, QualifiedCatalogAsset,
    VerifiedProductCatalog,
};
use clearra_accelerator_runtime::{
    active_identity, install, qualify_signed as qualify, remove as remove_active,
    QualifiedAccelerator,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::accelerator_download_transport::stream_release_asset;

const JOURNAL_SCHEMA: &str = "clearra.accelerator.local-activation.v1";
const JOURNAL: &str = "activation.journal";
const LOCK: &str = "store.lock";
const MAX_JOURNAL_BYTES: u64 = 256 * 1024;
const PROFILE_NAMES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalAssetState {
    Ready,
    NotLoaded,
    NotQualified,
    InvalidAsset,
    SnapshotMismatch,
}

impl LocalAssetState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotLoaded => "not_loaded",
            Self::NotQualified => "not_qualified",
            Self::InvalidAsset => "invalid_asset",
            Self::SnapshotMismatch => "snapshot_mismatch",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CatalogSummary {
    pub(crate) catalog_identity: [u8; 32],
    pub(crate) state: LocalAssetState,
    pub(crate) payload_bytes: Option<u64>,
    pub(crate) generation_identity: Option<[u8; 32]>,
}

#[derive(Clone, Debug)]
pub(crate) struct LocalAssetReport {
    pub(crate) state: LocalAssetState,
    pub(crate) installed: bool,
    pub(crate) payload_bytes: Option<u64>,
    pub(crate) generation_identity: Option<[u8; 32]>,
    pub(crate) catalog_identity: [u8; 32],
}

pub(crate) fn catalog_summary(
    kind: ProductCatalogKind,
    profile: &str,
) -> Result<CatalogSummary, &'static str> {
    let catalog = verified_catalog(kind)?;
    match catalog
        .profile(profile)
        .ok_or("accelerator: profile is absent from the catalog")?
    {
        CatalogProfileStatus::NotQualified => Ok(CatalogSummary {
            catalog_identity: catalog.catalog_identity(),
            state: LocalAssetState::NotQualified,
            payload_bytes: None,
            generation_identity: None,
        }),
        CatalogProfileStatus::Qualified(asset) => Ok(CatalogSummary {
            catalog_identity: catalog.catalog_identity(),
            state: LocalAssetState::Ready,
            payload_bytes: Some(asset.authority().payload_bytes()),
            generation_identity: Some(asset.authority().generation_identity()),
        }),
    }
}

pub(crate) fn status(
    kind: ProductCatalogKind,
    profile: &str,
    profile_root: &Path,
) -> Result<LocalAssetReport, &'static str> {
    let catalog = verified_catalog(kind)?;
    let catalog_identity = catalog.catalog_identity();
    let status = catalog
        .profile(profile)
        .ok_or("accelerator: profile is absent from the catalog")?;
    let CatalogProfileStatus::Qualified(asset) = status else {
        return Ok(LocalAssetReport {
            state: LocalAssetState::NotQualified,
            installed: profile_root.join(JOURNAL).is_file(),
            payload_bytes: None,
            generation_identity: None,
            catalog_identity,
        });
    };
    match load_active(kind, profile, profile_root, catalog_identity, asset) {
        Ok(Some(_)) => Ok(LocalAssetReport {
            state: LocalAssetState::Ready,
            installed: true,
            payload_bytes: Some(asset.authority().payload_bytes()),
            generation_identity: Some(asset.authority().generation_identity()),
            catalog_identity,
        }),
        Ok(None) => Ok(LocalAssetReport {
            state: LocalAssetState::NotLoaded,
            installed: false,
            payload_bytes: None,
            generation_identity: None,
            catalog_identity,
        }),
        Err("accelerator: installed snapshot does not match the embedded catalog") => {
            Ok(LocalAssetReport {
                state: LocalAssetState::SnapshotMismatch,
                installed: true,
                payload_bytes: None,
                generation_identity: None,
                catalog_identity,
            })
        }
        Err(_) => Ok(LocalAssetReport {
            state: LocalAssetState::InvalidAsset,
            installed: true,
            payload_bytes: None,
            generation_identity: None,
            catalog_identity,
        }),
    }
}

pub(crate) fn download(
    kind: ProductCatalogKind,
    profile: &str,
    profile_root: &Path,
) -> Result<LocalAssetReport, &'static str> {
    let cancelled = AtomicBool::new(false);
    download_observed(kind, profile, profile_root, &cancelled, &mut |_, _| {})
}

pub(crate) fn download_observed(
    kind: ProductCatalogKind,
    profile: &str,
    profile_root: &Path,
    cancelled: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<LocalAssetReport, &'static str> {
    validate_profile(profile)?;
    let catalog = verified_catalog(kind)?;
    let catalog_identity = catalog.catalog_identity();
    let CatalogProfileStatus::Qualified(asset) = catalog
        .profile(profile)
        .ok_or("accelerator: profile is absent from the catalog")?
    else {
        return Err("accelerator: this profile has no signed qualified Release asset");
    };
    ensure_real_directory(profile_root)?;
    let lock = open_lock(profile_root)?;
    lock.try_lock()
        .map_err(|_| "accelerator: profile store is in use")?;
    if cancelled.load(Ordering::Acquire) {
        return Err("accelerator: download cancelled");
    }

    let authority = asset.authority();
    let generation_name = generation_name(authority.generation_identity());
    let generation_root = profile_root.join(&generation_name);
    let payload_path = generation_root.join(payload_name(kind));
    if generation_root.exists() {
        reject_link(&generation_root, true)?;
        reject_link(&payload_path, false)?;
        let bytes = read_bounded(&payload_path, authority.payload_bytes())?;
        qualify(kind, profile, bytes.into(), asset)?;
    } else {
        let temporary = profile_root.join(format!("download-{}.tmp", std::process::id()));
        reject_link(&temporary, false)?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| "accelerator: could not create bounded download staging")?;
        let mut digest = Sha256::new();
        let mut transferred = 0_u64;
        let transfer = stream_release_asset(
            authority.asset_url(),
            authority.payload_bytes(),
            cancelled,
            &mut |chunk| {
                output
                    .write_all(chunk)
                    .map_err(|_| "accelerator: could not write download staging")?;
                digest.update(chunk);
                transferred += chunk.len() as u64;
                progress(transferred, authority.payload_bytes());
                Ok(())
            },
        );
        if let Err(error) = transfer {
            drop(output);
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        if output.sync_all().is_err() {
            drop(output);
            let _ = fs::remove_file(&temporary);
            return Err("accelerator: could not sync downloaded asset");
        }
        drop(output);
        let actual: [u8; 32] = digest.finalize().into();
        if actual != authority.payload_identity() {
            let _ = fs::remove_file(&temporary);
            return Err("accelerator: downloaded asset digest mismatch");
        }
        let qualification = read_bounded(&temporary, authority.payload_bytes())
            .and_then(|bytes| qualify(kind, profile, bytes.into(), asset).map(|_| ()));
        if let Err(error) = qualification {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        if cancelled.load(Ordering::Acquire) {
            let _ = fs::remove_file(&temporary);
            return Err("accelerator: download cancelled");
        }
        if fs::create_dir(&generation_root).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("accelerator: could not publish immutable generation directory");
        }
        if let Err(_error) = fs::rename(&temporary, &payload_path) {
            let _ = fs::remove_file(&temporary);
            let _ = fs::remove_dir(&generation_root);
            return Err("accelerator: could not publish immutable payload");
        }
        if cancelled.load(Ordering::Acquire) {
            let _ = fs::remove_file(&payload_path);
            let _ = fs::remove_dir(&generation_root);
            return Err("accelerator: download cancelled");
        }
    }
    if cancelled.load(Ordering::Acquire) {
        return Err("accelerator: download cancelled");
    }
    append_activation(
        profile_root,
        kind,
        profile,
        catalog_identity,
        asset,
        &generation_name,
    )?;
    cleanup_inactive_generations(profile_root, kind, &generation_name)?;
    status(kind, profile, profile_root)
}

pub(crate) fn activate_installed(
    kind: ProductCatalogKind,
    profile: &str,
    profile_root: &Path,
) -> LocalAssetState {
    let Ok(catalog) = verified_catalog(kind) else {
        return LocalAssetState::InvalidAsset;
    };
    let Some(status) = catalog.profile(profile) else {
        return LocalAssetState::InvalidAsset;
    };
    let CatalogProfileStatus::Qualified(asset) = status else {
        return LocalAssetState::NotQualified;
    };
    let expected = (
        asset.authority().generation_identity(),
        asset.authority().statement_identity(),
    );
    let active = active_identity(kind, profile);
    if active == Some(expected) {
        return LocalAssetState::Ready;
    }
    match load_active(
        kind,
        profile,
        profile_root,
        catalog.catalog_identity(),
        asset,
    ) {
        Ok(Some(asset)) => {
            if install(asset).is_ok() {
                LocalAssetState::Ready
            } else {
                LocalAssetState::InvalidAsset
            }
        }
        Ok(None) => LocalAssetState::NotLoaded,
        Err("accelerator: installed snapshot does not match the embedded catalog") => {
            LocalAssetState::SnapshotMismatch
        }
        Err(_) => LocalAssetState::InvalidAsset,
    }
}

pub(crate) fn remove(
    kind: ProductCatalogKind,
    profile: &str,
    profile_root: &Path,
) -> Result<bool, &'static str> {
    validate_profile(profile)?;
    if !profile_root.exists() {
        remove_active(kind, profile)?;
        return Ok(false);
    }
    reject_link(profile_root, true)?;
    let lock = open_lock(profile_root)?;
    lock.try_lock()
        .map_err(|_| "accelerator: profile store is in use")?;
    remove_active(kind, profile)?;
    remove_file_if_present(&profile_root.join(JOURNAL))?;
    cleanup_inactive_generations(profile_root, kind, "")?;
    Ok(true)
}

fn load_active(
    kind: ProductCatalogKind,
    profile: &str,
    profile_root: &Path,
    catalog_identity: [u8; 32],
    asset: &QualifiedCatalogAsset,
) -> Result<Option<QualifiedAccelerator>, &'static str> {
    if !profile_root.exists() {
        return Ok(None);
    }
    reject_link(profile_root, true)?;
    let journal = profile_root.join(JOURNAL);
    if !journal.exists() {
        return Ok(None);
    }
    reject_link(&journal, false)?;
    let pointer = read_last_activation(&journal)?;
    verify_pointer(&pointer, kind, profile, catalog_identity, asset)?;
    let directory = pointer["directory"]
        .as_str()
        .ok_or("accelerator: installed activation is invalid")?;
    let generation_root = profile_root.join(directory);
    reject_link(&generation_root, true)?;
    let payload = generation_root.join(payload_name(kind));
    reject_link(&payload, false)?;
    let bytes = read_bounded(&payload, asset.authority().payload_bytes())?;
    qualify(kind, profile, bytes.into(), asset).map(Some)
}

fn append_activation(
    profile_root: &Path,
    kind: ProductCatalogKind,
    profile: &str,
    catalog_identity: [u8; 32],
    asset: &QualifiedCatalogAsset,
    directory: &str,
) -> Result<(), &'static str> {
    let journal = profile_root.join(JOURNAL);
    reject_link(&journal, false)?;
    if journal
        .metadata()
        .ok()
        .is_some_and(|metadata| metadata.len() >= MAX_JOURNAL_BYTES.saturating_sub(4_096))
    {
        return Err(
            "accelerator: activation journal retention limit reached; remove and reinstall",
        );
    }
    let authority = asset.authority();
    let entry = json!({
        "catalog_identity": hex(catalog_identity),
        "directory": directory,
        "generation_identity": hex(authority.generation_identity()),
        "payload_bytes": authority.payload_bytes().to_string(),
        "payload_identity": hex(authority.payload_identity()),
        "product": kind.as_str(),
        "profile": profile,
        "qualification_identity": hex(authority.qualification_identity()),
        "rule_identity": hex(authority.rule_identity()),
        "schema": JOURNAL_SCHEMA,
        "statement_identity": hex(authority.statement_identity())
    });
    let mut encoded = serde_json::to_vec(&entry)
        .map_err(|_| "accelerator: could not encode activation record")?;
    encoded.push(b'\n');
    let mut output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal)
        .map_err(|_| "accelerator: could not open activation journal")?;
    output
        .write_all(&encoded)
        .and_then(|_| output.sync_all())
        .map_err(|_| "accelerator: could not commit activation record")
}

fn read_last_activation(path: &Path) -> Result<Value, &'static str> {
    let metadata = path
        .metadata()
        .map_err(|_| "accelerator: could not inspect activation journal")?;
    if metadata.len() == 0 || metadata.len() > MAX_JOURNAL_BYTES {
        return Err("accelerator: activation journal size invalid");
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|_| "accelerator: could not read activation journal")?;
    if !bytes.ends_with(b"\n") {
        return Err("accelerator: activation journal has an incomplete record");
    }
    let line = bytes[..bytes.len() - 1]
        .rsplit(|byte| *byte == b'\n')
        .next()
        .filter(|line| !line.is_empty())
        .ok_or("accelerator: activation journal is empty")?;
    serde_json::from_slice(line).map_err(|_| "accelerator: activation journal JSON invalid")
}

fn verify_pointer(
    pointer: &Value,
    kind: ProductCatalogKind,
    profile: &str,
    catalog_identity: [u8; 32],
    asset: &QualifiedCatalogAsset,
) -> Result<(), &'static str> {
    exact_keys(
        pointer
            .as_object()
            .ok_or("accelerator: installed activation is invalid")?,
        &[
            "catalog_identity",
            "directory",
            "generation_identity",
            "payload_bytes",
            "payload_identity",
            "product",
            "profile",
            "qualification_identity",
            "rule_identity",
            "schema",
            "statement_identity",
        ],
    )?;
    let authority = asset.authority();
    let generation = hex(authority.generation_identity());
    let expected_directory = generation_name(authority.generation_identity());
    if pointer["schema"] != JOURNAL_SCHEMA
        || pointer["product"] != kind.as_str()
        || pointer["profile"] != profile
        || pointer["catalog_identity"] != hex(catalog_identity)
        || pointer["directory"] != expected_directory
        || pointer["generation_identity"] != generation
        || pointer["rule_identity"] != hex(authority.rule_identity())
        || pointer["payload_identity"] != hex(authority.payload_identity())
        || pointer["payload_bytes"] != authority.payload_bytes().to_string()
        || pointer["qualification_identity"] != hex(authority.qualification_identity())
        || pointer["statement_identity"] != hex(authority.statement_identity())
    {
        return Err("accelerator: installed snapshot does not match the embedded catalog");
    }
    Ok(())
}

fn verified_catalog(kind: ProductCatalogKind) -> Result<VerifiedProductCatalog, &'static str> {
    embedded_catalog(kind).map_err(|_| "accelerator: embedded signed catalog is invalid")
}

fn validate_profile(profile: &str) -> Result<(), &'static str> {
    if PROFILE_NAMES.contains(&profile) {
        Ok(())
    } else {
        Err("accelerator: unsupported profile")
    }
}

pub(crate) fn ensure_real_directory(path: &Path) -> Result<(), &'static str> {
    // Validate every existing ancestor before the first write.  A post-create
    // check alone could already have traversed an attacker-controlled link.
    reject_link(path, false)?;
    fs::create_dir_all(path).map_err(|_| "accelerator: could not create profile store")?;
    reject_link(path, true)
}

pub(crate) fn validate_real_directory_if_present(path: &Path) -> Result<(), &'static str> {
    if path.exists() {
        reject_link(path, true)?;
    } else {
        reject_link(path, false)?;
    }
    Ok(())
}

fn open_lock(root: &Path) -> Result<File, &'static str> {
    let path = root.join(LOCK);
    reject_link(&path, false)?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| "accelerator: could not open profile-store lock")
}

fn read_bounded(path: &Path, exact_bytes: u64) -> Result<Vec<u8>, &'static str> {
    let metadata = path
        .metadata()
        .map_err(|_| "accelerator: installed payload is missing")?;
    if metadata.len() != exact_bytes {
        return Err("accelerator: installed payload length mismatch");
    }
    let mut bytes = Vec::with_capacity(exact_bytes as usize);
    File::open(path)
        .and_then(|file| file.take(exact_bytes + 1).read_to_end(&mut bytes))
        .map_err(|_| "accelerator: installed payload is unreadable")?;
    if bytes.len() as u64 != exact_bytes {
        return Err("accelerator: installed payload length mismatch");
    }
    Ok(bytes)
}

fn cleanup_inactive_generations(
    root: &Path,
    kind: ProductCatalogKind,
    keep: &str,
) -> Result<(), &'static str> {
    for entry in fs::read_dir(root).map_err(|_| "accelerator: could not list profile store")? {
        let entry = entry.map_err(|_| "accelerator: could not inspect profile store")?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if generation_directory_name(&name) && name != keep {
            let directory = entry.path();
            reject_link(&directory, true)?;
            remove_file_if_present(&directory.join(payload_name(kind)))?;
            fs::remove_dir(directory).map_err(|_| {
                "accelerator: inactive generation contains unexpected files or is in use"
            })?;
        }
    }
    Ok(())
}

fn remove_file_if_present(path: &Path) -> Result<(), &'static str> {
    reject_link(path, false)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("accelerator: could not remove managed file"),
    }
}

fn reject_link(path: &Path, require_directory: bool) -> Result<(), &'static str> {
    for candidate in path.ancestors() {
        match fs::symlink_metadata(candidate) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err("accelerator: linked storage paths are not allowed");
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err("accelerator: reparse storage paths are not allowed");
                    }
                }
                if candidate == path && require_directory && !metadata.is_dir() {
                    return Err("accelerator: expected a real directory");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("accelerator: storage path could not be inspected"),
        }
    }
    Ok(())
}

fn payload_name(kind: ProductCatalogKind) -> &'static str {
    match kind {
        ProductCatalogKind::ExactLegalBoard => "asset.cllb",
        ProductCatalogKind::BoardConditionedReachability => "asset.clbr",
    }
}

fn generation_name(identity: [u8; 32]) -> String {
    format!("generation-{}", hex(identity))
}

fn generation_directory_name(value: &str) -> bool {
    value.strip_prefix("generation-").is_some_and(|identity| {
        identity.len() == 64
            && identity
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn exact_keys(object: &Map<String, Value>, keys: &[&str]) -> Result<(), &'static str> {
    if object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key)) {
        Ok(())
    } else {
        Err("accelerator: installed activation has unknown or missing fields")
    }
}

pub(crate) fn hex(identity: [u8; 32]) -> String {
    identity.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn profile_root(base: &Path, kind: ProductCatalogKind, profile: &str) -> PathBuf {
    let namespace = match kind {
        ProductCatalogKind::ExactLegalBoard => "legal-board-v1",
        ProductCatalogKind::BoardConditionedReachability => "conditioned-reachability-v1",
    };
    base.join(namespace).join(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_unqualified_catalog_never_opens_network_or_store() {
        for kind in [
            ProductCatalogKind::ExactLegalBoard,
            ProductCatalogKind::BoardConditionedReachability,
        ] {
            for profile in PROFILE_NAMES {
                let summary = catalog_summary(kind, profile).unwrap();
                assert_eq!(summary.state, LocalAssetState::NotQualified);
                assert_eq!(summary.payload_bytes, None);
            }
        }
    }

    #[test]
    fn managed_generation_names_are_narrow() {
        assert!(generation_directory_name(&format!(
            "generation-{}",
            "1".repeat(64)
        )));
        for value in [
            "generation-1",
            "generation-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "candidate-1111111111111111111111111111111111111111111111111111111111111111",
            "generation-1111111111111111111111111111111111111111111111111111111111111111/other",
        ] {
            assert!(!generation_directory_name(value));
        }
    }
}
