//! Tiny filesystem/publication contracts, not qualification of synthetic assets.
//! No transport process or remote asset is used by these tests.
use super::*;

struct Fixture {
    directory: PathBuf,
    allowed_root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = std::env::var_os("CLEARRA_FOCUSED_TEST_OUTPUT_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("_local/artifacts/accelerator-store-tests")
            });
        let root = absolute_fixture_root(&root);
        let sequence = REPAIR_PUBLICATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = root.join(format!("fixture-{}-{sequence}-{label}", std::process::id()));
        ensure_real_directory(&root).expect("declared focused-test root");
        fs::create_dir(&directory).expect("fresh exact fixture directory");
        Self {
            directory,
            allowed_root: root,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Resolve this exact fresh child before recursive fixture cleanup. No
        // workspace, profile store, asset directory, or broad root is removed.
        if reject_link(&self.directory, true).is_err() {
            return;
        }
        let Ok(directory) = fs::canonicalize(&self.directory) else {
            return;
        };
        let Ok(root) = fs::canonicalize(&self.allowed_root) else {
            return;
        };
        if directory.parent() == Some(root.as_path())
            && directory
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("fixture-"))
        {
            let _ = fs::remove_dir_all(directory);
        }
    }
}

fn absolute_fixture_root(root: &Path) -> PathBuf {
    // Only an actual repository-declared local-artifacts root is accepted;
    // an environment variable is not unrestricted write/delete authority.
    assert!(root.is_absolute(), "test output root must be absolute");
    let mut normalized = PathBuf::new();
    for component in root.components() {
        match component {
            std::path::Component::ParentDir | std::path::Component::CurDir => {
                panic!("test output traversal is forbidden")
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    assert!(normalized.is_absolute());
    assert_eq!(normalized.file_name().unwrap(), "accelerator-store-tests");
    reject_link(&normalized, false).expect("unlinked focused-test path");
    let repository = normalized
        .ancestors()
        .find(|candidate| {
            candidate
                .join("config/clearra-management.v1.json")
                .is_file()
                && candidate.join("Cargo.toml").is_file()
        })
        .expect("focused output must belong to a Clearra repository");
    let policy_path = repository.join("config/clearra-management.v1.json");
    assert!(policy_path.metadata().unwrap().len() <= 256 * 1024);
    let policy: Value = serde_json::from_slice(&fs::read(policy_path).unwrap()).unwrap();
    assert!(policy["repository_roots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["id"] == "local-artifacts" && entry["path"] == "_local/artifacts"));
    assert!(normalized.starts_with(repository.join("_local/artifacts")));
    normalized
}

#[test]
fn repair_directory_is_a_narrow_same_signed_generation_installation() {
    let identity = [0x12; 32];
    let original = generation_name(identity);
    let repaired = format!("{original}-repair-23-1");
    assert!(generation_directory_matches(&original, identity));
    assert!(generation_directory_matches(&repaired, identity));
    assert!(!generation_directory_matches(&repaired, [0x13; 32]));
    for suffix in [
        "/outside",
        "-repair-0-1",
        "-repair-23-0",
        "-repair-023-1",
        "-repair-23-01",
        "-repair-23-1/other",
        "-repair-23-1-extra",
        "-repair-4294967296-1",
        "-repair-23-18446744073709551616",
    ] {
        assert!(
            !generation_directory_name(&format!("{original}{suffix}")),
            "{suffix}"
        );
    }
}

#[test]
fn repair_publication_preserves_old_generation_and_pointer_until_commit() {
    let fixture = Fixture::new("commit");
    let old = fixture.directory.join(generation_name([1; 32]));
    fs::create_dir(&old).unwrap();
    fs::write(old.join("asset.cllb"), b"old-verified-owner").unwrap();
    fs::write(fixture.directory.join(JOURNAL), b"old-activation\n").unwrap();
    let staged = fixture.directory.join("download.tmp");
    fs::write(&staged, b"new-verified-payload").unwrap();
    let name = format!("{}-repair-23-1", generation_name([2; 32]));
    let publication = publish_verified_generation(
        ProductCatalogKind::ExactLegalBoard,
        &fixture.directory,
        &name,
        &staged,
    )
    .unwrap();
    assert_eq!(
        fs::read(old.join("asset.cllb")).unwrap(),
        b"old-verified-owner"
    );
    assert_eq!(
        fs::read(fixture.directory.join(JOURNAL)).unwrap(),
        b"old-activation\n"
    );
    assert_eq!(
        fs::read(&publication.payload).unwrap(),
        b"new-verified-payload"
    );
    // This test checks filesystem staging only. A real product call must first
    // pass the signed catalog, digest and full qualifier before this point.
    publication.commit();
    assert!(fixture.directory.join(&name).join("asset.cllb").is_file());
}

#[test]
fn repair_cancel_before_journal_attempt_reclaims_only_its_new_sibling() {
    let fixture = Fixture::new("cancel");
    let old = fixture.directory.join(generation_name([3; 32]));
    fs::create_dir(&old).unwrap();
    fs::write(old.join("asset.cllr"), b"previous-owner").unwrap();
    fs::write(fixture.directory.join(JOURNAL), b"previous-activation\n").unwrap();
    let staged = fixture.directory.join("download.tmp");
    fs::write(&staged, b"validated-staging").unwrap();
    let name = format!("{}-repair-23-1", generation_name([3; 32]));
    let publication = publish_verified_generation(
        ProductCatalogKind::BoardConditionedReachability,
        &fixture.directory,
        &name,
        &staged,
    )
    .unwrap();
    drop(publication);
    assert!(!fixture.directory.join(name).exists());
    assert_eq!(fs::read(old.join("asset.cllr")).unwrap(), b"previous-owner");
    assert_eq!(
        fs::read(fixture.directory.join(JOURNAL)).unwrap(),
        b"previous-activation\n"
    );
}

#[test]
fn repair_never_overwrites_an_existing_sibling() {
    let fixture = Fixture::new("collision");
    let name = format!("{}-repair-23-1", generation_name([4; 32]));
    let directory = fixture.directory.join(&name);
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("asset.cllb"), b"existing").unwrap();
    let staged = fixture.directory.join("download.tmp");
    fs::write(&staged, b"replacement").unwrap();
    assert!(publish_verified_generation(
        ProductCatalogKind::ExactLegalBoard,
        &fixture.directory,
        &name,
        &staged
    )
    .is_err());
    assert_eq!(fs::read(directory.join("asset.cllb")).unwrap(), b"existing");
    assert!(!staged.exists());
}

#[test]
fn corrupt_signed_generation_allows_explicit_transfer_but_failed_repair_preserves_it() {
    let fixture = Fixture::new("failed-transfer");
    let kind = ProductCatalogKind::ExactLegalBoard;
    let catalog = verified_catalog(kind).unwrap();
    let CatalogProfileStatus::Qualified(asset) = catalog.profile("srs").unwrap() else {
        panic!("declared qualified fixture catalog");
    };
    let directory = fixture
        .directory
        .join(generation_name(asset.authority().generation_identity()));
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join(payload_name(kind)), b"truncated").unwrap();
    fs::write(fixture.directory.join(JOURNAL), b"previous-pointer\n").unwrap();
    let mut calls = 0;
    let result = download_observed_with_transport(
        kind,
        "srs",
        &fixture.directory,
        &AtomicBool::new(false),
        &mut |_, _| {},
        |_url, _length, _cancelled, _sink| {
            calls += 1;
            Err("test-only transfer failure")
        },
    );
    assert_eq!(result.unwrap_err(), "test-only transfer failure");
    assert_eq!(
        calls, 1,
        "old code stopped before reaching the explicit transfer"
    );
    assert_eq!(
        fs::read(directory.join(payload_name(kind))).unwrap(),
        b"truncated"
    );
    assert_eq!(
        fs::read(fixture.directory.join(JOURNAL)).unwrap(),
        b"previous-pointer\n"
    );
    assert!(!fixture
        .directory
        .join(format!("download-{}.tmp", std::process::id()))
        .exists());
}

#[test]
fn repair_digest_failure_cannot_publish_or_activate_any_payload() {
    let fixture = Fixture::new("wrong-digest");
    let kind = ProductCatalogKind::BoardConditionedReachability;
    let catalog = verified_catalog(kind).unwrap();
    let CatalogProfileStatus::Qualified(asset) = catalog.profile("srs-plus").unwrap() else {
        panic!("declared qualified fixture catalog");
    };
    let directory = fixture
        .directory
        .join(generation_name(asset.authority().generation_identity()));
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join(payload_name(kind)), b"corrupt").unwrap();
    fs::write(fixture.directory.join(JOURNAL), b"old-pointer\n").unwrap();
    let result = download_observed_with_transport(
        kind,
        "srs-plus",
        &fixture.directory,
        &AtomicBool::new(false),
        &mut |_, _| {},
        |_url, length, _cancelled, sink| {
            // Exact-length zero bytes exercise digest rejection without reading
            // or downloading the qualified payload. Buffer size stays constant.
            let chunk = [0u8; 4096];
            let mut remaining = length;
            while remaining > 0 {
                let bytes = usize::try_from(remaining.min(chunk.len() as u64)).unwrap();
                sink(&chunk[..bytes])?;
                remaining -= bytes as u64;
            }
            Ok(())
        },
    );
    assert_eq!(
        result.unwrap_err(),
        "accelerator: downloaded asset digest mismatch"
    );
    assert_eq!(
        fs::read(directory.join(payload_name(kind))).unwrap(),
        b"corrupt"
    );
    assert_eq!(
        fs::read(fixture.directory.join(JOURNAL)).unwrap(),
        b"old-pointer\n"
    );
    let names = fs::read_dir(&fixture.directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(!names
        .iter()
        .any(|name| name.contains("-repair-") || name.ends_with(".tmp")));
}

#[test]
fn repair_staging_enforces_signed_length_before_write_independently_of_transport() {
    let fixture = Fixture::new("size");
    let kind = ProductCatalogKind::BoardConditionedReachability;
    let mut progress = Vec::new();
    let short = download_observed_with_transport(
        kind,
        "srs",
        &fixture.directory,
        &AtomicBool::new(false),
        &mut |bytes, _| progress.push(bytes),
        |_url, _length, _cancelled, sink| sink(&[1]),
    );
    assert_eq!(
        short.unwrap_err(),
        "accelerator: response is shorter than signed byte length"
    );
    assert_eq!(progress, [1]);
    progress.clear();
    let overflow = download_observed_with_transport(
        kind,
        "srs",
        &fixture.directory,
        &AtomicBool::new(false),
        &mut |bytes, _| progress.push(bytes),
        |_url, length, _cancelled, sink| {
            let chunk = [0u8; 4096];
            let mut remaining = length;
            while remaining > 0 {
                let bytes = usize::try_from(remaining.min(chunk.len() as u64)).unwrap();
                sink(&chunk[..bytes])?;
                remaining -= bytes as u64;
            }
            sink(&[1])
        },
    );
    assert_eq!(
        overflow.unwrap_err(),
        "accelerator: response exceeds signed byte length"
    );
    let catalog = verified_catalog(kind).unwrap();
    let CatalogProfileStatus::Qualified(asset) = catalog.profile("srs").unwrap() else {
        panic!("qualified test catalog");
    };
    assert_eq!(
        progress.last().copied(),
        Some(asset.authority().payload_bytes())
    );
    assert!(!fixture.directory.join(JOURNAL).exists());
    assert!(!fixture
        .directory
        .join(format!("download-{}.tmp", std::process::id()))
        .exists());
}
