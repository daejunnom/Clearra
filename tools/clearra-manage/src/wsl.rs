use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions, TryLockError};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::policy::{Policy, WslEntry};
use crate::runtime::{self, RunOptions, RunOutcome};
use crate::storage;
use crate::{Error, Result};

pub fn run_entry(
    repository: &Path,
    policy: &Policy,
    entry_name: &str,
    arguments: &[OsString],
) -> Result<RunOutcome> {
    if !cfg!(windows) {
        return Err(Error::runtime(
            "managed WSL is available only from a Windows host",
        ));
    }
    let contract = policy
        .runtime_policy
        .wsl
        .entrypoints
        .get(entry_name)
        .ok_or_else(|| Error::usage(format!("unregistered WSL entrypoint: {entry_name}")))?
        .clone();
    reject_control_characters(arguments)?;
    let _lease = WslLease::acquire(repository, policy)?;
    let run_tag = format!("{}-{}", std::process::id(), unix_nanos());
    let mut guest_arguments =
        normalize_entry_arguments(repository, policy, &contract, entry_name, arguments)?;
    let temporary_archive = contract
        .requires_source
        .then(|| source_archive(repository, policy, &run_tag))
        .transpose()?;
    if let Some(archive) = &temporary_archive {
        let mounted_archive = windows_to_wsl(&archive.path)?;
        guest_arguments.splice(
            0..0,
            [
                OsString::from("--source-archive"),
                OsString::from(mounted_archive),
                OsString::from("--source-digest"),
                OsString::from(archive.digest.as_str()),
            ],
        );
    }

    let wsl = wsl_executable();
    let distro = &policy.runtime_policy.wsl.distribution;
    let session = windows_to_wsl(&repository.join("scripts/runtime/clearra-wsl-session.sh"))?;
    let guest = windows_to_wsl(&repository.join("scripts/runtime/clearra-wsl-guest.sh"))?;
    let profile = policy.profile(&contract.profile)?;
    let requested_timeout = contract.timeout_seconds.unwrap_or(profile.timeout_seconds);
    let cleanup_budget = 60u64.min(requested_timeout.saturating_sub(1).max(1));
    let guest_timeout = requested_timeout.saturating_sub(cleanup_budget).max(1);
    let host_snapshot = runtime::current_memory_snapshot()?;
    let admission = runtime::calculate_admission(policy, &contract.profile, host_snapshot)?;
    // The guest forbids swap. Its cgroup must therefore remain within host
    // physical capacity even though the Windows outer Job Object may use the
    // larger commit limit backed by the user's page file. Neither boundary
    // depends on free memory at WSL session start.
    let hard_limit = guest_hard_limit(
        host_snapshot.physical_total_bytes,
        admission.physical_pressure_reserve_bytes,
        admission.hard_limit_bytes,
        admission.minimum_bytes,
    )
    .ok_or_else(|| {
        Error::runtime(format!(
            "E_CLEARRA_MEMORY_ADMISSION_DENIED: WSL profile {} exceeds stable guest physical capacity",
            contract.profile
        ))
    })?;
    let unit = format!("clearra-rust-{run_tag}");
    let digest = marker_digest(policy)?;
    let tool = |name: &str| {
        policy
            .toolchains
            .get(name)
            .cloned()
            .ok_or_else(|| Error::policy(format!("missing toolchain version: {name}")))
    };
    let mut command = vec![
        OsString::from(wsl),
        OsString::from("-d"),
        OsString::from(distro),
        OsString::from("--user"),
        OsString::from("root"),
        OsString::from("--exec"),
        OsString::from("/bin/bash"),
        OsString::from(session),
        OsString::from("--unit"),
        OsString::from(unit),
        OsString::from("--memory-max"),
        OsString::from(hard_limit.to_string()),
        OsString::from("--tasks-max"),
        OsString::from(profile.maximum_descendant_processes.to_string()),
        OsString::from("--runtime-max"),
        OsString::from(guest_timeout.to_string()),
        OsString::from("--grace"),
        OsString::from(profile.termination_grace_seconds.to_string()),
        OsString::from("--guest"),
        OsString::from(guest),
        OsString::from("--mode"),
        OsString::from("runtime"),
        OsString::from("--run-as"),
        OsString::from("clearra"),
        OsString::from("--entry"),
        OsString::from(entry_name),
        OsString::from("--marker-digest"),
        OsString::from(digest),
        OsString::from("--node-version"),
        OsString::from(tool("node")?),
        OsString::from("--npm-version"),
        OsString::from(tool("npm")?),
        OsString::from("--pnpm-version"),
        OsString::from(tool("pnpm")?),
        OsString::from("--rust-version"),
        OsString::from(tool("rust")?),
        OsString::from("--cargo-version"),
        OsString::from(tool("cargo")?),
        OsString::from("--wasm-bindgen-version"),
        OsString::from(tool("wasm_bindgen")?),
        OsString::from("--"),
    ];
    command.extend(guest_arguments);

    let result = runtime::run(
        repository,
        policy,
        RunOptions {
            producer: "wsl".to_owned(),
            profile: contract.profile,
            timeout_seconds: Some(requested_timeout),
            command,
            keep_stdin_open: true,
            extra_env: BTreeMap::new(),
            echo: true,
        },
    );
    let termination = terminate_dedicated(policy);
    drop(temporary_archive);
    termination?;
    result
}

fn guest_hard_limit(
    host_physical_total: u64,
    physical_pressure_reserve: u64,
    outer_hard_limit: u64,
    declared_minimum: u64,
) -> Option<u64> {
    let guest_cap = host_physical_total.saturating_sub(physical_pressure_reserve);
    let limit = outer_hard_limit.min(guest_cap);
    (limit >= declared_minimum).then_some(limit)
}

fn normalize_entry_arguments(
    repository: &Path,
    policy: &Policy,
    contract: &WslEntry,
    entry: &str,
    arguments: &[OsString],
) -> Result<Vec<OsString>> {
    let mut values = arguments.to_vec();
    for option in &contract.output_path_options {
        let index = unique_option_value(&values, option)?;
        let verified = storage::verify(repository, policy, Path::new(&values[index]), false, None)?;
        values[index] = OsString::from(windows_to_wsl(&verified.path)?);
    }
    for option in &contract.input_path_options {
        let index = unique_option_value(&values, option)?;
        let path = PathBuf::from(&values[index]);
        if storage::is_secret_path(policy, &path) {
            return Err(Error::storage(
                "prohibited credential path blocked; contents were not inspected",
            ));
        }
        let canonical =
            fs::canonicalize(&path).map_err(|error| Error::io("canonicalize WSL input", error))?;
        values[index] = OsString::from(windows_to_wsl(&canonical)?);
    }
    if entry == "oracle-local-layers-v080" {
        values.splice(
            0..0,
            [
                OsString::from("--repository-root"),
                OsString::from(windows_to_wsl(repository)?),
            ],
        );
    }
    if entry == "posix-syntax-audit" {
        let mut cursor = 0;
        while cursor < values.len() {
            if values[cursor] == "--host-path" {
                cursor += 1;
                if cursor >= values.len() {
                    return Err(Error::usage("--host-path requires a value"));
                }
                let path = fs::canonicalize(PathBuf::from(&values[cursor]))
                    .map_err(|error| Error::io("canonicalize syntax-audit path", error))?;
                if !path.starts_with(repository) || storage::is_secret_path(policy, &path) {
                    return Err(Error::storage(
                        "POSIX syntax audit accepts only non-secret repository files",
                    ));
                }
                values[cursor] = OsString::from(windows_to_wsl(&path)?);
            }
            cursor += 1;
        }
    }
    Ok(values)
}

fn unique_option_value(arguments: &[OsString], option: &str) -> Result<usize> {
    let indexes = arguments
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (value == option).then_some(index + 1))
        .collect::<Vec<_>>();
    if indexes.len() != 1 || indexes[0] >= arguments.len() {
        return Err(Error::usage(format!(
            "WSL entry requires exactly one {option} <path>"
        )));
    }
    Ok(indexes[0])
}

fn source_archive(repository: &Path, policy: &Policy, run_tag: &str) -> Result<WslSourceArchive> {
    let root = temporary_root()?.join("wsl-source").join(run_tag);
    let root = storage::verify(repository, policy, &root, false, None)?.path;
    // Own only a newly created run directory. Early failures in listing,
    // filtering, tar, or hashing must also release it.
    let staging = OwnedTemporaryDirectory::create(root)?;
    let listing = Command::new("git")
        .args(["-C", &repository.to_string_lossy(), "ls-files", "-z"])
        .output()
        .map_err(|error| Error::io("list tracked WSL source", error))?;
    if !listing.status.success() {
        return Err(Error::runtime(
            "git ls-files failed while preparing WSL source",
        ));
    }
    for raw in listing
        .stdout
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(String::from_utf8_lossy(raw).into_owned());
        if storage::is_secret_path(policy, &path) {
            return Err(Error::storage(
                "prohibited credential path blocked; contents were not inspected",
            ));
        }
    }
    let list = staging.0.join("tracked-files.zlist");
    fs::write(&list, &listing.stdout).map_err(|error| Error::io("write WSL source list", error))?;
    let path = staging.0.join("source.tar.gz");
    let status = Command::new("tar")
        .current_dir(repository)
        .arg("-czf")
        .arg(&path)
        .arg("--null")
        .arg("-T")
        .arg(&list)
        .status()
        .map_err(|error| Error::io("create WSL source archive", error))?;
    if !status.success() || !path.is_file() {
        return Err(Error::runtime("tar failed while preparing WSL source"));
    }
    let digest = sha256_file(&path)?;
    Ok(WslSourceArchive {
        _staging: staging,
        path,
        digest,
    })
}

fn marker_digest(policy: &Policy) -> Result<String> {
    let material = serde_json::json!({
        "toolchains": &policy.toolchains,
        "sources": &policy.toolchain_sources,
    });
    let bytes = serde_json::to_vec(&material)
        .map_err(|error| Error::policy(format!("serialize toolchain identity: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|error| Error::io("open source archive", error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| Error::io("hash source archive", error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn terminate_dedicated(policy: &Policy) -> Result<()> {
    let wsl = &policy.runtime_policy.wsl;
    if wsl.allow_shutdown
        || wsl.distribution
            != policy
                .runtime_policy
                .process_tree_contract
                .wsl_terminate_distribution
    {
        return Err(Error::policy(
            "WSL termination ownership contract is invalid",
        ));
    }
    let executable = wsl_executable();
    if !dedicated_is_running(&executable, &wsl.distribution)? {
        return Ok(());
    }
    let status = Command::new(&executable)
        .args(["--terminate", &wsl.distribution])
        .status()
        .map_err(|error| Error::io("terminate dedicated WSL distribution", error))?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if !dedicated_is_running(&executable, &wsl.distribution)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(Error::runtime(format!(
        "E_CLEARRA_WSL_TERMINATION_FAILED: exit={status} distribution is still running"
    )))
}

fn dedicated_is_running(executable: &Path, distribution: &str) -> Result<bool> {
    let running = Command::new(executable)
        .args(["--list", "--running", "--quiet"])
        .output()
        .map_err(|error| Error::io("verify WSL termination", error))?;
    if !running.status.success() {
        return Err(Error::runtime(
            "E_CLEARRA_WSL_TERMINATION_FAILED: could not list running distributions",
        ));
    }
    let text = decode_wsl_output(&running.stdout);
    Ok(text
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case(distribution)))
}

fn decode_wsl_output(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xff, 0xfe])
        || bytes
            .iter()
            .skip(1)
            .step_by(2)
            .take(8)
            .all(|value| *value == 0)
    {
        let values = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .filter(|value| *value != 0xfeff)
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&values)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn windows_to_wsl(path: &Path) -> Result<String> {
    let value = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let rendered = value.to_string_lossy();
    let rendered = rendered.strip_prefix(r"\\?\").unwrap_or(&rendered);
    if rendered.to_ascii_lowercase().starts_with(r"unc\") {
        return Err(Error::storage(
            "WSL host paths on UNC shares are not supported",
        ));
    }
    let bytes = rendered.as_bytes();
    if bytes.len() < 3 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return Err(Error::storage(format!(
            "WSL host path must use a drive-qualified Windows path: {rendered}"
        )));
    }
    let drive = (bytes[0] as char).to_ascii_lowercase();
    let rest = rendered[2..].replace('\\', "/");
    Ok(format!(
        "/mnt/{drive}{}",
        if rest.starts_with('/') {
            rest
        } else {
            format!("/{rest}")
        }
    ))
}

fn reject_control_characters(arguments: &[OsString]) -> Result<()> {
    if arguments.iter().any(|value| {
        let value = value.to_string_lossy();
        value.contains('\0') || value.contains('\r') || value.contains('\n')
    }) {
        return Err(Error::usage(
            "WSL arguments may not contain control characters",
        ));
    }
    Ok(())
}

fn wsl_executable() -> PathBuf {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32/wsl.exe")
}

fn temporary_root() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| Error::runtime("LOCALAPPDATA is unavailable"))?;
    Ok(PathBuf::from(base).join("Clearra/tmp"))
}

fn unix_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

const WSL_LOCK_FILENAME: &str = "Clearra-Build.lock";
const LEGACY_WSL_LEASE_FILENAME: &str = "Clearra-Build.lease";

struct WslLease {
    // Closing the handle releases the OS lock. The path must stay in
    // place so the next manager cannot acquire a different file object.
    _file: fs::File,
}

struct WslSourceArchive {
    _staging: OwnedTemporaryDirectory,
    path: PathBuf,
    digest: String,
}

struct OwnedTemporaryDirectory(PathBuf);

impl OwnedTemporaryDirectory {
    fn create(path: PathBuf) -> Result<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| Error::storage("WSL source staging has no parent"))?;
        fs::create_dir_all(parent)
            .map_err(|error| Error::io("create WSL source staging root", error))?;
        fs::create_dir(&path)
            .map_err(|error| Error::io("create unique WSL source staging", error))?;
        Ok(Self(path))
    }
}

impl Drop for OwnedTemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl WslLease {
    fn acquire(repository: &Path, policy: &Policy) -> Result<Self> {
        let directory = runtime::state_root()?.join("wsl");
        let path = directory.join(WSL_LOCK_FILENAME);
        let path = storage::verify(repository, policy, &path, false, None)?.path;
        let lease = Self::lock_at(&path)?;
        terminate_dedicated(policy)?;
        Ok(lease)
    }

    fn lock_at(path: &Path) -> Result<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| Error::storage("WSL lock has no parent"))?;
        fs::create_dir_all(parent).map_err(|error| Error::io("create WSL lock root", error))?;
        match fs::symlink_metadata(path) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(Error::storage("permanent WSL lock is not a regular file"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io("inspect permanent WSL lock", error)),
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_SHARE_READ: u32 = 1;
            const FILE_SHARE_WRITE: u32 = 2;
            // Let another caller open the same file and observe contention;
            // forbid renaming or replacing that file while a handle is live.
            options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }
        let file = options
            .open(path)
            .map_err(|error| Error::io("open permanent WSL lock", error))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(Error::runtime(
                    "E_CLEARRA_WSL_SESSION_BUSY: dedicated distribution already has an owner",
                ));
            }
            Err(TryLockError::Error(error)) => {
                return Err(Error::io("acquire permanent WSL lock", error));
            }
        }

        // The obsolete PID-lease implementation does not observe this lock.
        // No mixed-version execution is supported. Any legacy lease is an
        // explicit migration blocker; we never infer staleness or terminate
        // on its behalf, even when it is empty, unreadable, or a dangling link.
        match fs::symlink_metadata(parent.join(LEGACY_WSL_LEASE_FILENAME)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io("verify legacy WSL lease absence", error)),
            Ok(_) => {
                return Err(Error::runtime(
                    "E_CLEARRA_WSL_SESSION_BUSY: stop legacy managers and verify the old WSL lease before migration",
                ));
            }
        }
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Policy;

    fn test_staging(label: &str) -> OwnedTemporaryDirectory {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("repository root");
        let policy = Policy::load(repository).expect("load policy");
        let path = repository.join("_local/state/wsl-tests").join(format!(
            "{label}-{}-{}",
            std::process::id(),
            unix_nanos()
        ));
        let path = storage::verify(repository, &policy, &path, false, None)
            .expect("managed test path")
            .path;
        OwnedTemporaryDirectory::create(path).expect("create test staging")
    }

    #[test]
    fn permanent_wsl_lock_rejects_a_second_handle_and_remains_for_reuse() {
        let staging = test_staging("lock");
        let path = staging.0.join(WSL_LOCK_FILENAME);
        let owner = WslLease::lock_at(&path).expect("first lock");
        let Err(busy) = WslLease::lock_at(&path) else {
            panic!("second handle acquired the lock");
        };
        assert!(busy.to_string().contains("E_CLEARRA_WSL_SESSION_BUSY"));
        drop(owner);
        assert!(path.is_file(), "the lock path is permanent");
        WslLease::lock_at(&path).expect("same file can be locked again");
    }

    #[test]
    fn empty_legacy_wsl_lease_is_not_interpreted_as_stale() {
        let staging = test_staging("legacy-lock");
        let path = staging.0.join(WSL_LOCK_FILENAME);
        let legacy = staging.0.join(LEGACY_WSL_LEASE_FILENAME);
        fs::File::create_new(&legacy).expect("empty ownership marker");
        let Err(error) = WslLease::lock_at(&path) else {
            panic!("lock accepted while legacy owner may still be active");
        };
        assert!(error.to_string().contains("E_CLEARRA_WSL_SESSION_BUSY"));
        assert!(legacy.is_file(), "the legacy lease was not removed");
    }

    #[test]
    fn source_staging_refuses_existing_directory_and_cleans_on_early_error() {
        let staging = test_staging("source-staging");
        let existing = staging.0.join("existing");
        fs::create_dir(&existing).expect("unowned directory");
        assert!(OwnedTemporaryDirectory::create(existing.clone()).is_err());
        assert!(existing.is_dir(), "existing paths are never acquired");

        let temporary = staging.0.join("failing-archive");
        let fail_after_create = || -> Result<()> {
            let _staging = OwnedTemporaryDirectory::create(temporary.clone())?;
            fs::File::create_new(temporary.join("tracked-files.zlist"))
                .map_err(|error| Error::io("simulate partial archive", error))?;
            Err(Error::runtime("simulated archive error"))
        };
        assert!(fail_after_create().is_err());
        assert!(!temporary.exists(), "partial staging was not cleaned");
    }

    #[test]
    fn utf16_wsl_listing_is_decoded() {
        let bytes = [
            0xff, 0xfe, b'U', 0, b'b', 0, b'u', 0, b'n', 0, b't', 0, b'u', 0, b'\n', 0,
        ];
        assert!(decode_wsl_output(&bytes).contains("Ubuntu"));
    }

    #[test]
    fn marker_digest_stays_compatible_with_the_python_v1_distribution() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let policy = Policy::load(&root).expect("load policy");
        assert_eq!(
            marker_digest(&policy).expect("marker digest"),
            "ae944b9b6162e6861e9fec2613d3ee01bd785096abb4b57c815ca19d28c91fdb"
        );
    }

    #[test]
    fn control_characters_are_rejected() {
        assert!(reject_control_characters(&[OsString::from("ok\nno")]).is_err());
    }

    #[test]
    fn guest_cgroup_uses_total_physical_capacity_not_start_availability() {
        const GIB: u64 = 1024 * 1024 * 1024;
        assert_eq!(
            guest_hard_limit(16 * GIB, GIB / 2, 31 * GIB, 3 * GIB),
            Some(16 * GIB - GIB / 2)
        );
        assert_eq!(
            guest_hard_limit(16 * GIB, GIB / 2, 6 * GIB, 3 * GIB),
            Some(6 * GIB)
        );
        assert_eq!(guest_hard_limit(2 * GIB, GIB / 2, 31 * GIB, 3 * GIB), None);
    }
}
