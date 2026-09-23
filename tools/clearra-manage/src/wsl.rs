use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
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
    let _lease = WslLease::acquire(policy)?;
    let run_tag = format!("{}-{}", std::process::id(), unix_nanos());
    let mut temporary_archive = None;
    let mut guest_arguments =
        normalize_entry_arguments(repository, policy, &contract, entry_name, arguments)?;
    if contract.requires_source {
        let (archive, digest) = source_archive(repository, policy, &run_tag)?;
        temporary_archive = archive
            .parent()
            .map(|path| OwnedTemporaryDirectory(path.to_path_buf()));
        let mounted_archive = windows_to_wsl(&archive)?;
        guest_arguments.splice(
            0..0,
            [
                OsString::from("--source-archive"),
                OsString::from(mounted_archive),
                OsString::from("--source-digest"),
                OsString::from(digest),
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

fn source_archive(repository: &Path, policy: &Policy, run_tag: &str) -> Result<(PathBuf, String)> {
    let root = temporary_root()?.join("wsl-source").join(run_tag);
    fs::create_dir_all(&root).map_err(|error| Error::io("create WSL source staging", error))?;
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
    let list = root.join("tracked-files.zlist");
    fs::write(&list, &listing.stdout).map_err(|error| Error::io("write WSL source list", error))?;
    let archive = root.join("source.tar.gz");
    let status = Command::new("tar")
        .current_dir(repository)
        .arg("-czf")
        .arg(&archive)
        .arg("--null")
        .arg("-T")
        .arg(&list)
        .status()
        .map_err(|error| Error::io("create WSL source archive", error))?;
    if !status.success() || !archive.is_file() {
        return Err(Error::runtime("tar failed while preparing WSL source"));
    }
    let digest = sha256_file(&archive)?;
    Ok((archive, digest))
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

struct WslLease {
    path: PathBuf,
    owner: String,
}

struct OwnedTemporaryDirectory(PathBuf);

impl Drop for OwnedTemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl WslLease {
    fn acquire(policy: &Policy) -> Result<Self> {
        let path = runtime::state_root()?.join("wsl/Clearra-Build.lease");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| Error::io("create WSL lease root", error))?;
        }
        let owner = format!("{}\n{}\n", std::process::id(), unix_nanos());
        for attempt in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(owner.as_bytes())
                        .map_err(|error| Error::io("write WSL lease", error))?;
                    let lease = Self {
                        path: path.clone(),
                        owner,
                    };
                    terminate_dedicated(policy)?;
                    return Ok(lease);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let live = fs::read_to_string(&path)
                        .ok()
                        .and_then(|value| value.lines().next()?.parse::<u32>().ok())
                        .is_some_and(runtime::pid_alive);
                    if live || attempt > 0 {
                        return Err(Error::runtime(
                            "E_CLEARRA_WSL_SESSION_BUSY: dedicated distribution already has an owner",
                        ));
                    }
                    terminate_dedicated(policy)?;
                    fs::remove_file(&path)
                        .map_err(|error| Error::io("remove stale WSL lease", error))?;
                }
                Err(error) => return Err(Error::io("acquire WSL lease", error)),
            }
        }
        Err(Error::runtime(
            "E_CLEARRA_WSL_SESSION_BUSY: could not acquire dedicated distribution lease",
        ))
    }
}

impl Drop for WslLease {
    fn drop(&mut self) {
        if fs::read_to_string(&self.path).ok().as_deref() == Some(&self.owner) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Policy;

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
