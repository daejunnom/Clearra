use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::policy::{Policy, ResourceProfile};
use crate::storage;
use crate::{Error, Result};

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MemorySnapshot {
    pub physical_total_bytes: u64,
    pub physical_available_bytes: u64,
    pub commit_limit_bytes: u64,
    pub commit_available_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub profile: String,
    pub minimum_bytes: u64,
    pub hard_limit_bytes: u64,
    pub critical_physical_reserve_bytes: u64,
    pub critical_commit_reserve_bytes: u64,
    pub snapshot: MemorySnapshot,
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub producer: String,
    pub profile: String,
    pub timeout_seconds: Option<u64>,
    pub command: Vec<OsString>,
    pub keep_stdin_open: bool,
    pub extra_env: BTreeMap<OsString, OsString>,
    pub echo: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunOutcome {
    pub run_id: String,
    pub producer: String,
    pub profile: String,
    pub command: Vec<String>,
    pub return_code: i32,
    pub reason: String,
    pub error_code: Option<String>,
    pub duration_ms: u128,
    pub timeout_seconds: u64,
    pub output_bytes: u64,
    pub output_limit_bytes: u64,
    pub peak_memory_bytes: Option<u64>,
    pub descendant_processes_at_exit: Option<u32>,
    pub process_tree_stopped: bool,
    pub admission: Admission,
    pub memory_pressure_observed: bool,
    pub gc_request_written: bool,
    pub gc_acknowledged: bool,
    pub automatic_retry: bool,
    pub receipt: PathBuf,
}

pub fn audit(repository: &Path, policy: &Policy) -> Result<serde_json::Value> {
    let snapshot = memory_snapshot()?;
    let profiles = policy
        .resource_profiles
        .keys()
        .map(|name| {
            let admission = calculate_admission(policy, name, snapshot).map(|value| json!(value));
            (
                name.clone(),
                match admission {
                    Ok(value) => value,
                    Err(error) => json!({"error": error.to_string()}),
                },
            )
        })
        .collect::<serde_json::Map<_, _>>();
    Ok(json!({
        "schema_id": "clearra.runtime-audit.v2",
        "repository": repository,
        "snapshot": snapshot,
        "profiles": profiles,
        "scope": ["process-memory", "process-tree", "timeout", "output-limit", "wsl-lifecycle"],
        "excluded_scope": ["git", "filesystem-reading", "dependency-management", "toolchain-installation", "package-publishing", "general-process-registration"],
    }))
}

pub fn calculate_admission(
    policy: &Policy,
    profile_name: &str,
    snapshot: MemorySnapshot,
) -> Result<Admission> {
    let profile = policy.profile(profile_name)?;
    let pressure = &policy.runtime_policy.memory_pressure;
    let critical_physical = pressure.critical_physical_reserve_mib * MIB;
    let critical_commit = pressure.critical_commit_reserve_mib * MIB;
    if snapshot.physical_available_bytes < critical_physical
        || snapshot.commit_available_bytes < critical_commit
    {
        return Err(Error::runtime(
            "E_CLEARRA_MEMORY_ADMISSION_DENIED: critical host reserve is unavailable",
        ));
    }
    let physical_cap = snapshot
        .physical_total_bytes
        .saturating_sub(pressure.physical_reserve_mib * MIB);
    let commit_cap = snapshot
        .commit_limit_bytes
        .saturating_sub(pressure.commit_reserve_mib * MIB);
    let dynamic_cap = physical_cap.min(commit_cap);
    let hard_limit = profile
        .maximum_memory_mib
        .map(|value| value * MIB)
        .unwrap_or(dynamic_cap)
        .min(dynamic_cap);
    let minimum = profile.minimum_memory_mib * MIB;
    if hard_limit < minimum {
        return Err(Error::runtime(format!(
            "E_CLEARRA_MEMORY_ADMISSION_DENIED: profile {profile_name} requires {minimum} bytes but only {hard_limit} bytes can be bounded safely"
        )));
    }
    Ok(Admission {
        profile: profile_name.to_owned(),
        minimum_bytes: minimum,
        hard_limit_bytes: hard_limit,
        critical_physical_reserve_bytes: critical_physical,
        critical_commit_reserve_bytes: critical_commit,
        snapshot,
    })
}

pub fn run(repository: &Path, policy: &Policy, options: RunOptions) -> Result<RunOutcome> {
    if options.command.is_empty() {
        return Err(Error::usage("runtime run requires a command after --"));
    }
    let profile = policy.profile(&options.profile)?.clone();
    let timeout = options.timeout_seconds.unwrap_or(profile.timeout_seconds);
    if (profile.explicit_timeout_required || profile.explicit_lease_required)
        && options.timeout_seconds.is_none()
    {
        return Err(Error::usage(format!(
            "profile {} requires an explicit --timeout",
            options.profile
        )));
    }
    if timeout == 0 || timeout > profile.maximum_timeout_seconds {
        return Err(Error::usage(format!(
            "timeout for {} must be between 1 and {} seconds",
            options.profile, profile.maximum_timeout_seconds
        )));
    }
    if policy.runtime_policy.memory_pressure.automatic_retry {
        return Err(Error::policy("automatic retry must remain disabled"));
    }

    let snapshot = memory_snapshot()?;
    let admission = calculate_admission(policy, &options.profile, snapshot)?;
    let run_id = run_id();
    let state = state_root()?.join("runtime");
    fs::create_dir_all(&state).map_err(|error| Error::io("create runtime state", error))?;
    let _slot = RuntimeSlot::acquire(policy, &profile, &run_id, &state)?;
    let gc_root = state.join("gc");
    fs::create_dir_all(&gc_root).map_err(|error| Error::io("create GC state", error))?;
    let gc_request = gc_root.join(format!("{run_id}.request"));
    let gc_ack = gc_root.join(format!("{run_id}.ack"));

    let mut command = Command::new(&options.command[0]);
    command
        .args(&options.command[1..])
        .current_dir(repository)
        .stdin(if options.keep_stdin_open {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("CLEARRA_RUNTIME_SUPERVISED", "1")
        .env("CLEARRA_RUNTIME_PROFILE", &options.profile)
        .env(
            "CLEARRA_RUNTIME_HARD_LIMIT_BYTES",
            admission.hard_limit_bytes.to_string(),
        )
        .env(
            "CLEARRA_RUNTIME_GC_PROTOCOL",
            &policy
                .runtime_policy
                .memory_pressure
                .cooperative_gc_protocol,
        )
        .env("CLEARRA_RUNTIME_GC_REQUEST_PATH", &gc_request)
        .env("CLEARRA_RUNTIME_GC_ACK_PATH", &gc_ack)
        .env(
            "CLEARRA_MANAGED_OUTPUT_ROOTS",
            storage::managed_roots_json(repository, policy)?,
        );
    for (key, value) in &options.extra_env {
        command.env(key, value);
    }
    platform_prepare_command(&mut command);

    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| Error::io("start supervised command", error))?;
    let mut stdin_lease = if options.keep_stdin_open {
        child.stdin.take()
    } else {
        None
    };
    let containment = match Containment::attach(
        &child,
        admission.hard_limit_bytes,
        profile.maximum_descendant_processes,
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };

    let total_output = Arc::new(AtomicU64::new(0));
    let output_exceeded = Arc::new(AtomicBool::new(false));
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::runtime("child stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::runtime("child stderr unavailable"))?;
    let stdout_thread = collect_output(
        stdout,
        false,
        options.echo,
        profile.output_limit_bytes,
        Arc::clone(&total_output),
        Arc::clone(&output_exceeded),
    );
    let stderr_thread = collect_output(
        stderr,
        true,
        options.echo,
        profile.output_limit_bytes,
        Arc::clone(&total_output),
        Arc::clone(&output_exceeded),
    );

    let timeout_at = started + Duration::from_secs(timeout);
    let sample_interval = Duration::from_secs_f64(
        policy
            .runtime_policy
            .memory_pressure
            .sample_interval_seconds
            .max(0.25),
    );
    let mut next_sample = Instant::now() + sample_interval;
    let mut pressure_seen = false;
    let mut gc_written = false;
    let mut gc_acknowledged = false;
    let mut observed_peak = 0u64;
    let mut forced_reason: Option<(&'static str, &'static str)> = None;
    let status: ExitStatus;

    loop {
        if let Some(value) = child
            .try_wait()
            .map_err(|error| Error::io("poll supervised command", error))?
        {
            status = value;
            break;
        }
        if output_exceeded.load(Ordering::Relaxed) {
            forced_reason = Some(("output-limit", "E_CLEARRA_PROCESS_OUTPUT_LIMIT"));
            terminate_tree(&mut child, &containment, profile.termination_grace_seconds);
            status = child
                .wait()
                .map_err(|error| Error::io("wait after output limit", error))?;
            break;
        }
        if Instant::now() >= timeout_at {
            forced_reason = Some(("timeout", "E_CLEARRA_PROCESS_TIMEOUT"));
            terminate_tree(&mut child, &containment, profile.termination_grace_seconds);
            status = child
                .wait()
                .map_err(|error| Error::io("wait after timeout", error))?;
            break;
        }
        if Instant::now() >= next_sample {
            next_sample += sample_interval;
            if let Some(current) = containment.current_memory_bytes() {
                observed_peak = observed_peak.max(current);
                if current > admission.hard_limit_bytes {
                    forced_reason = Some(("memory-limit", "E_CLEARRA_PROCESS_MEMORY_LIMIT"));
                    terminate_tree(&mut child, &containment, profile.termination_grace_seconds);
                    status = child
                        .wait()
                        .map_err(|error| Error::io("wait after memory limit", error))?;
                    break;
                }
            }
            if containment
                .active_processes()
                .is_some_and(|count| count > profile.maximum_descendant_processes)
            {
                forced_reason = Some(("process-limit", "E_CLEARRA_PROCESS_COUNT_LIMIT"));
                terminate_tree(&mut child, &containment, profile.termination_grace_seconds);
                status = child
                    .wait()
                    .map_err(|error| Error::io("wait after process limit", error))?;
                break;
            }
            let current = memory_snapshot()?;
            let pressure = &policy.runtime_policy.memory_pressure;
            let physical_low =
                current.physical_available_bytes < pressure.physical_reserve_mib * MIB;
            let commit_low = current.commit_available_bytes < pressure.commit_reserve_mib * MIB;
            if physical_low || commit_low {
                pressure_seen = true;
                if !gc_written {
                    fs::write(&gc_request, b"clearra.memory-pressure.v1\n")
                        .map_err(|error| Error::io("write cooperative GC request", error))?;
                    gc_written = true;
                }
                thread::sleep(Duration::from_secs_f64(
                    pressure.recovery_grace_seconds.max(0.0),
                ));
                gc_acknowledged |= gc_ack.is_file();
                let recovered = memory_snapshot()?;
                if recovered.physical_available_bytes < pressure.physical_reserve_mib * MIB
                    || recovered.commit_available_bytes < pressure.commit_reserve_mib * MIB
                {
                    forced_reason =
                        Some(("memory-pressure", "E_CLEARRA_MEMORY_PRESSURE_FAIL_CLOSE"));
                    terminate_tree(&mut child, &containment, profile.termination_grace_seconds);
                    status = child
                        .wait()
                        .map_err(|error| Error::io("wait after memory pressure", error))?;
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    drop(stdin_lease.take());
    let stdout = stdout_thread
        .join()
        .map_err(|_| Error::runtime("stdout collector panicked"))?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| Error::runtime("stderr collector panicked"))?;
    let peak = containment
        .peak_memory_bytes()
        .or((observed_peak > 0).then_some(observed_peak));
    let active = containment.active_processes();
    let mut tree_stopped = active.unwrap_or(0) == 0;
    if !tree_stopped {
        containment.force_terminate();
        thread::sleep(Duration::from_millis(100));
        tree_stopped = containment.active_processes().unwrap_or(0) == 0;
        if forced_reason.is_none() {
            forced_reason = Some(("tree-not-stopped", "E_CLEARRA_PROCESS_TREE_NOT_STOPPED"));
        }
    }
    let mut return_code = status.code().unwrap_or(125);
    let (reason, error_code) = if let Some((reason, code)) = forced_reason {
        if return_code == 0 {
            return_code = 125;
        }
        (reason.to_owned(), Some(code.to_owned()))
    } else if return_code == 0 {
        ("normal".to_owned(), None)
    } else if peak.is_some_and(|value| value >= admission.hard_limit_bytes.saturating_mul(98) / 100)
    {
        (
            "memory-limit".to_owned(),
            Some("E_CLEARRA_PROCESS_MEMORY_LIMIT".to_owned()),
        )
    } else {
        ("nonzero".to_owned(), None)
    };

    let receipts = receipt_root()?;
    fs::create_dir_all(&receipts).map_err(|error| Error::io("create receipt root", error))?;
    let receipt = receipts.join(format!("{run_id}-runtime.json"));
    let command_strings = redact_command(&options.command);
    let outcome = RunOutcome {
        run_id,
        producer: options.producer,
        profile: options.profile,
        command: command_strings,
        return_code,
        reason,
        error_code,
        duration_ms: started.elapsed().as_millis(),
        timeout_seconds: timeout,
        output_bytes: total_output.load(Ordering::Relaxed),
        output_limit_bytes: profile.output_limit_bytes,
        peak_memory_bytes: peak,
        descendant_processes_at_exit: active,
        process_tree_stopped: tree_stopped,
        admission,
        memory_pressure_observed: pressure_seen,
        gc_request_written: gc_written,
        gc_acknowledged,
        automatic_retry: false,
        receipt: receipt.clone(),
    };
    let value = json!({
        "schema_id": "clearra.runtime-receipt.v2",
        "outcome": outcome,
        "stdout_tail": String::from_utf8_lossy(&stdout),
        "stderr_tail": String::from_utf8_lossy(&stderr),
    });
    fs::write(
        &receipt,
        serde_json::to_vec_pretty(&value)
            .map_err(|error| Error::runtime(format!("serialize receipt: {error}")))?,
    )
    .map_err(|error| Error::io("write runtime receipt", error))?;
    let _ = fs::remove_file(gc_request);
    let _ = fs::remove_file(gc_ack);
    Ok(outcome)
}

fn collect_output<R: Read + Send + 'static>(
    mut reader: R,
    stderr: bool,
    echo: bool,
    limit: u64,
    total: Arc<AtomicU64>,
    exceeded: Arc<AtomicBool>,
) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut retained = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let count = match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };
            let previous = total.fetch_add(count as u64, Ordering::Relaxed);
            if previous + count as u64 > limit {
                exceeded.store(true, Ordering::Relaxed);
            }
            let remaining = limit.saturating_sub(previous) as usize;
            retained.extend_from_slice(&buffer[..count.min(remaining)]);
            if echo {
                if stderr {
                    let _ = std::io::stderr().write_all(&buffer[..count]);
                    let _ = std::io::stderr().flush();
                } else {
                    let _ = std::io::stdout().write_all(&buffer[..count]);
                    let _ = std::io::stdout().flush();
                }
            }
        }
        retained
    })
}

fn terminate_tree(child: &mut Child, containment: &Containment, grace_seconds: u64) {
    containment.request_termination(child);
    let deadline = Instant::now() + Duration::from_secs(grace_seconds);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some()
            && containment.active_processes().unwrap_or(0) == 0
        {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    containment.force_terminate();
    let _ = child.kill();
}

fn redact_command(command: &[OsString]) -> Vec<String> {
    let mut redact_next = false;
    command
        .iter()
        .map(|value| {
            let rendered = value.to_string_lossy();
            if redact_next {
                redact_next = false;
                return "<redacted>".to_owned();
            }
            let lower = rendered.to_ascii_lowercase();
            if lower.contains("token=") || lower.contains("password=") || lower.contains("api_key=")
            {
                return "<redacted>".to_owned();
            }
            if matches!(
                lower.as_str(),
                "--token"
                    | "--password"
                    | "--api-key"
                    | "--key-file"
                    | "-i"
                    | "--identity-file"
                    | "--credential-file"
                    | "--credentials"
                    | "--service-account"
                    | "--env-file"
            ) {
                redact_next = true;
            }
            rendered.into_owned()
        })
        .collect()
}

fn run_id() -> String {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{value}-{}", std::process::id())
}

pub fn state_root() -> Result<PathBuf> {
    if cfg!(windows) {
        let value = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| Error::runtime("LOCALAPPDATA is unavailable"))?;
        Ok(PathBuf::from(value).join("Clearra/state"))
    } else if let Some(value) = std::env::var_os("XDG_STATE_HOME") {
        Ok(PathBuf::from(value).join("Clearra"))
    } else {
        let home = std::env::var_os("HOME").ok_or_else(|| Error::runtime("HOME is unavailable"))?;
        Ok(PathBuf::from(home).join(".local/state/Clearra"))
    }
}

fn receipt_root() -> Result<PathBuf> {
    Ok(state_root()?.join("receipts/rust-v2"))
}

struct RuntimeSlot {
    path: PathBuf,
    owner: String,
}

impl RuntimeSlot {
    fn acquire(
        policy: &Policy,
        profile: &ResourceProfile,
        run_id: &str,
        state: &Path,
    ) -> Result<Self> {
        let class = policy
            .runtime_policy
            .parallel_admission
            .classes
            .get(&profile.concurrency_class)
            .ok_or_else(|| Error::policy("missing concurrency class"))?;
        let root = state.join("slots");
        fs::create_dir_all(&root).map_err(|error| Error::io("create runtime slots", error))?;
        for index in 0..class.maximum_parallel {
            let path = root.join(format!("{}-{index}.lock", profile.concurrency_class));
            let owner = format!("{}\n{}\n", std::process::id(), run_id);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(owner.as_bytes())
                        .map_err(|error| Error::io("write runtime slot", error))?;
                    return Ok(Self { path, owner });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = fs::read_to_string(&path)
                        .ok()
                        .and_then(|value| value.lines().next()?.parse::<u32>().ok())
                        .is_some_and(|pid| !pid_alive(pid));
                    if stale {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                }
                Err(error) => return Err(Error::io("acquire runtime slot", error)),
            }
        }
        Err(Error::runtime(format!(
            "E_CLEARRA_RUNTIME_SLOT_BUSY: concurrency class {} is occupied",
            profile.concurrency_class
        )))
    }
}

impl Drop for RuntimeSlot {
    fn drop(&mut self) {
        if fs::read_to_string(&self.path).ok().as_deref() == Some(&self.owner) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(windows)]
fn memory_snapshot() -> Result<MemorySnapshot> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut value: MEMORYSTATUSEX = unsafe { zeroed() };
    value.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut value) } == 0 {
        return Err(Error::runtime("GlobalMemoryStatusEx failed"));
    }
    Ok(MemorySnapshot {
        physical_total_bytes: value.ullTotalPhys,
        physical_available_bytes: value.ullAvailPhys,
        commit_limit_bytes: value.ullTotalPageFile,
        commit_available_bytes: value.ullAvailPageFile,
    })
}

pub fn current_memory_snapshot() -> Result<MemorySnapshot> {
    memory_snapshot()
}

#[cfg(target_os = "linux")]
fn memory_snapshot() -> Result<MemorySnapshot> {
    let contents = fs::read_to_string("/proc/meminfo")
        .map_err(|error| Error::io("read /proc/meminfo", error))?;
    let mut values = BTreeMap::new();
    for line in contents.lines() {
        if let Some((name, rest)) = line.split_once(':') {
            if let Some(value) = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
            {
                values.insert(name, value * 1024);
            }
        }
    }
    let total = *values
        .get("MemTotal")
        .ok_or_else(|| Error::runtime("MemTotal is unavailable"))?;
    let available = *values
        .get("MemAvailable")
        .ok_or_else(|| Error::runtime("MemAvailable is unavailable"))?;
    let swap_total = *values.get("SwapTotal").unwrap_or(&0);
    let swap_free = *values.get("SwapFree").unwrap_or(&0);
    Ok(MemorySnapshot {
        physical_total_bytes: total,
        physical_available_bytes: available,
        commit_limit_bytes: total.saturating_add(swap_total),
        commit_available_bytes: available.saturating_add(swap_free),
    })
}

#[cfg(not(any(windows, target_os = "linux")))]
fn memory_snapshot() -> Result<MemorySnapshot> {
    Err(Error::runtime(
        "memory snapshot is supported only on Windows and Linux",
    ))
}

#[cfg(windows)]
fn platform_prepare_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0000_0200); // CREATE_NEW_PROCESS_GROUP
}

#[cfg(unix)]
fn platform_prepare_command(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(any(windows, unix)))]
fn platform_prepare_command(_: &mut Command) {}

#[cfg(windows)]
struct Containment {
    job: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl Containment {
    fn attach(child: &Child, hard_limit: u64, maximum_processes: u32) -> Result<Self> {
        use std::mem::{size_of, zeroed};
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(Error::runtime("CreateJobObjectW failed"));
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_JOB_MEMORY
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        limits.BasicLimitInformation.ActiveProcessLimit = maximum_processes;
        limits.JobMemoryLimit = hard_limit as usize;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err(Error::runtime("SetInformationJobObject failed"));
        }
        let assigned = unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as _) };
        if assigned == 0 {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err(Error::runtime("AssignProcessToJobObject failed"));
        }
        Ok(Self { job })
    }

    fn request_termination(&self, child: &mut Child) {
        let _ = child.kill();
    }

    fn force_terminate(&self) {
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 125) };
    }

    fn peak_memory_bytes(&self) -> Option<u64> {
        use std::mem::{size_of, zeroed};
        use windows_sys::Win32::System::JobObjects::{
            JobObjectExtendedLimitInformation, QueryInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        };
        let mut value: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        let ok = unsafe {
            QueryInformationJobObject(
                self.job,
                JobObjectExtendedLimitInformation,
                (&mut value as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(value.PeakJobMemoryUsed as u64)
    }

    fn current_memory_bytes(&self) -> Option<u64> {
        self.peak_memory_bytes()
    }

    fn active_processes(&self) -> Option<u32> {
        use std::mem::{size_of, zeroed};
        use windows_sys::Win32::System::JobObjects::{
            JobObjectBasicAccountingInformation, QueryInformationJobObject,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        };
        let mut value: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        let ok = unsafe {
            QueryInformationJobObject(
                self.job,
                JobObjectBasicAccountingInformation,
                (&mut value as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(value.ActiveProcesses)
    }
}

#[cfg(unix)]
impl Drop for Containment {
    fn drop(&mut self) {
        self.force_terminate();
    }
}

#[cfg(windows)]
impl Drop for Containment {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.job) };
    }
}

#[cfg(unix)]
struct Containment {
    process_group: i32,
    hard_limit: u64,
    maximum_processes: u32,
}

#[cfg(unix)]
impl Containment {
    fn attach(child: &Child, hard_limit: u64, maximum_processes: u32) -> Result<Self> {
        Ok(Self {
            process_group: child.id() as i32,
            hard_limit,
            maximum_processes,
        })
    }

    fn request_termination(&self, _: &mut Child) {
        unsafe { libc::kill(-self.process_group, libc::SIGTERM) };
    }

    fn force_terminate(&self) {
        unsafe { libc::kill(-self.process_group, libc::SIGKILL) };
    }

    fn peak_memory_bytes(&self) -> Option<u64> {
        let _ = self.hard_limit;
        None
    }

    fn current_memory_bytes(&self) -> Option<u64> {
        process_group_usage(self.process_group).map(|(memory, _)| memory)
    }

    fn active_processes(&self) -> Option<u32> {
        let _ = self.maximum_processes;
        process_group_usage(self.process_group).map(|(_, count)| count)
    }
}

#[cfg(not(any(windows, unix)))]
struct Containment;

#[cfg(not(any(windows, unix)))]
impl Containment {
    fn attach(_: &Child, _: u64, _: u32) -> Result<Self> {
        Err(Error::runtime("process containment is unsupported"))
    }
    fn request_termination(&self, child: &mut Child) {
        let _ = child.kill();
    }
    fn force_terminate(&self) {}
    fn peak_memory_bytes(&self) -> Option<u64> {
        None
    }
    fn current_memory_bytes(&self) -> Option<u64> {
        None
    }
    fn active_processes(&self) -> Option<u32> {
        None
    }
}

#[cfg(target_os = "linux")]
fn process_group_usage(process_group: i32) -> Option<(u64, u32)> {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return None;
    }
    let mut bytes = 0u64;
    let mut count = 0u32;
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let stat = match fs::read_to_string(entry.path().join("stat")) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let tail = match stat.rsplit_once(") ") {
            Some((_, tail)) => tail,
            None => continue,
        };
        let fields = tail.split_whitespace().collect::<Vec<_>>();
        if fields.get(2).and_then(|value| value.parse::<i32>().ok()) != Some(process_group) {
            continue;
        }
        count = count.saturating_add(1);
        if let Ok(statm) = fs::read_to_string(entry.path().join("statm")) {
            if let Some(resident) = statm
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<u64>().ok())
            {
                bytes = bytes.saturating_add(resident.saturating_mul(page_size as u64));
            }
        }
    }
    Some((bytes, count))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn process_group_usage(process_group: i32) -> Option<(u64, u32)> {
    let alive = unsafe { libc::kill(-process_group, 0) } == 0;
    Some((0, u32::from(alive)))
}

#[cfg(windows)]
pub(crate) fn pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
    true
}

#[cfg(unix)]
pub(crate) fn pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(any(windows, unix)))]
pub(crate) fn pid_alive(_: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ResourceProfile {
        ResourceProfile {
            minimum_memory_mib: 256,
            maximum_memory_mib: Some(512),
            timeout_seconds: 300,
            maximum_timeout_seconds: 300,
            termination_grace_seconds: 1,
            maximum_descendant_processes: 32,
            output_limit_bytes: 1024,
            hard_containment_required: false,
            oom_policy: "fail-no-retry".to_owned(),
            admission_basis: "runtime-pressure".to_owned(),
            start_admission: "critical-reserve".to_owned(),
            memory_pressure_action: "gc-then-fail-close".to_owned(),
            concurrency_class: "control".to_owned(),
            explicit_timeout_required: false,
            explicit_lease_required: false,
        }
    }

    #[test]
    fn redaction_does_not_record_secret_values() {
        let values = vec![
            OsString::from("tool"),
            OsString::from("--token"),
            OsString::from("secret"),
        ];
        assert_eq!(
            redact_command(&values),
            vec!["tool", "--token", "<redacted>"]
        );
        let identity = vec![
            OsString::from("ssh"),
            OsString::from("-i"),
            OsString::from("private-key-path"),
        ];
        assert_eq!(redact_command(&identity), vec!["ssh", "-i", "<redacted>"]);
    }

    #[test]
    fn resource_profile_fixture_retains_no_retry() {
        assert_eq!(profile().oom_policy, "fail-no-retry");
    }
}
