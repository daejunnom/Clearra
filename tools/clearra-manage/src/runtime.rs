// SRP rationale: this module has one change reason: supervised process-tree
// execution with admission, bounded resources and verifiable receipts.
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

use crate::policy::{MemoryPressurePolicy, Policy, ResourceProfile};
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
    pub maximum_bytes: Option<u64>,
    pub hard_limit_bytes: u64,
    pub capacity_basis: String,
    pub start_admission_mode: String,
    pub physical_pressure_reserve_bytes: u64,
    pub commit_pressure_reserve_bytes: Option<u64>,
    pub critical_physical_reserve_bytes: u64,
    pub critical_commit_reserve_bytes: Option<u64>,
    pub snapshot: MemorySnapshot,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MemoryPressureStatus {
    pub physical_reserve_bytes: u64,
    pub commit_reserve_bytes: Option<u64>,
    pub critical_physical_reserve_bytes: u64,
    pub critical_commit_reserve_bytes: Option<u64>,
    pub physical_low: bool,
    pub commit_low: bool,
    pub critical: bool,
}

impl MemoryPressureStatus {
    fn under_pressure(self) -> bool {
        self.physical_low || self.commit_low
    }
}

#[derive(Debug, Clone, Copy)]
enum HostMemoryModel {
    WindowsCommit,
    Physical,
}

fn host_memory_model() -> HostMemoryModel {
    if cfg!(windows) {
        HostMemoryModel::WindowsCommit
    } else {
        HostMemoryModel::Physical
    }
}

/// Small one-shot stdin payload. Debug output must never reveal its bytes.
pub struct BoundedStdin(Vec<u8>);

impl BoundedStdin {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl std::fmt::Debug for BoundedStdin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("BoundedStdin([redacted])")
    }
}

impl Drop for BoundedStdin {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Debug)]
pub struct RunOptions {
    pub producer: String,
    pub profile: String,
    pub timeout_seconds: Option<u64>,
    pub command: Vec<OsString>,
    pub keep_stdin_open: bool,
    pub stdin_payload: Option<BoundedStdin>,
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
    pub memory_pressure_events: u64,
    pub memory_pressure_recoveries: u64,
    pub cooperative_gc_requests: u64,
    pub cooperative_gc_acknowledgements: u64,
    pub memory_pressure_checks: u64,
    pub last_memory_pressure_snapshot: Option<MemorySnapshot>,
    pub last_memory_pressure_status: Option<MemoryPressureStatus>,
    pub automatic_retry: bool,
    pub receipt: PathBuf,
}

pub fn audit(repository: &Path, policy: &Policy) -> Result<serde_json::Value> {
    let snapshot = memory_snapshot()?;
    let pressure = memory_pressure_status(
        &policy.runtime_policy.memory_pressure,
        snapshot,
        host_memory_model(),
    );
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
        "memory_pressure": pressure,
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
    calculate_admission_for_profile(
        profile_name,
        profile,
        pressure,
        snapshot,
        host_memory_model(),
    )
}

fn calculate_admission_for_profile(
    profile_name: &str,
    profile: &ResourceProfile,
    pressure: &MemoryPressurePolicy,
    snapshot: MemorySnapshot,
    model: HostMemoryModel,
) -> Result<Admission> {
    let status = memory_pressure_status(pressure, snapshot, model);
    let mut reasons = Vec::new();
    if snapshot.physical_available_bytes < status.critical_physical_reserve_bytes {
        reasons.push("physical-available");
    }
    if status
        .critical_commit_reserve_bytes
        .is_some_and(|reserve| snapshot.commit_available_bytes < reserve)
    {
        reasons.push("commit-available");
    }
    if !reasons.is_empty() {
        return Err(Error::runtime(format!(
            "E_CLEARRA_MEMORY_ADMISSION_DENIED: profile={profile_name} start_admission=critical-reserve reasons={} available_bytes={} critical_physical_reserve_bytes={} commit_available_bytes={} critical_commit_reserve_bytes={:?}",
            reasons.join(","),
            snapshot.physical_available_bytes,
            status.critical_physical_reserve_bytes,
            snapshot.commit_available_bytes,
            status.critical_commit_reserve_bytes,
        )));
    }

    let minimum = profile.minimum_memory_mib * MIB;
    let maximum = profile.maximum_memory_mib.map(|value| value * MIB);
    // A configured maximum is a stable process-tree bound. It is not a
    // reservation from the start snapshot: external processes can change the
    // available memory immediately after launch, so host pressure is handled
    // continuously below instead.
    let hard_limit = maximum.unwrap_or_else(|| match model {
        HostMemoryModel::WindowsCommit => snapshot
            .commit_limit_bytes
            .saturating_sub(status.commit_reserve_bytes.unwrap_or(0)),
        HostMemoryModel::Physical => snapshot
            .physical_total_bytes
            .saturating_sub(status.physical_reserve_bytes),
    });
    if hard_limit < minimum {
        return Err(Error::runtime(format!(
            "E_CLEARRA_MEMORY_ADMISSION_DENIED: profile={profile_name} declared_minimum_bytes={minimum} stable_capacity_bytes={hard_limit}"
        )));
    }
    Ok(Admission {
        profile: profile_name.to_owned(),
        minimum_bytes: minimum,
        maximum_bytes: maximum,
        hard_limit_bytes: hard_limit,
        capacity_basis: "runtime-pressure".to_owned(),
        start_admission_mode: "critical-reserve".to_owned(),
        physical_pressure_reserve_bytes: status.physical_reserve_bytes,
        commit_pressure_reserve_bytes: status.commit_reserve_bytes,
        critical_physical_reserve_bytes: status.critical_physical_reserve_bytes,
        critical_commit_reserve_bytes: status.critical_commit_reserve_bytes,
        snapshot,
    })
}

fn reserve_from_floor_and_fraction(total: u64, floor_mib: u64, fraction: f64) -> u64 {
    let proportional = ((total as f64) * fraction).floor() as u64;
    (floor_mib * MIB).max(proportional)
}

fn memory_pressure_status(
    pressure: &MemoryPressurePolicy,
    snapshot: MemorySnapshot,
    model: HostMemoryModel,
) -> MemoryPressureStatus {
    let physical_reserve = reserve_from_floor_and_fraction(
        snapshot.physical_total_bytes,
        pressure.physical_reserve_mib,
        pressure.physical_reserve_fraction,
    );
    let critical_physical = pressure.critical_physical_reserve_mib * MIB;
    let (commit_reserve, critical_commit) = match model {
        HostMemoryModel::WindowsCommit => (
            Some(reserve_from_floor_and_fraction(
                snapshot.commit_limit_bytes,
                pressure.commit_reserve_mib,
                pressure.commit_reserve_fraction,
            )),
            Some(pressure.critical_commit_reserve_mib * MIB),
        ),
        HostMemoryModel::Physical => (None, None),
    };
    let physical_low = snapshot.physical_available_bytes < physical_reserve;
    let commit_low =
        commit_reserve.is_some_and(|reserve| snapshot.commit_available_bytes < reserve);
    let critical = snapshot.physical_available_bytes < critical_physical
        || critical_commit.is_some_and(|reserve| snapshot.commit_available_bytes < reserve);
    MemoryPressureStatus {
        physical_reserve_bytes: physical_reserve,
        commit_reserve_bytes: commit_reserve,
        critical_physical_reserve_bytes: critical_physical,
        critical_commit_reserve_bytes: critical_commit,
        physical_low,
        commit_low,
        critical,
    }
}

fn reset_gc_channel(request: &Path, acknowledgement: &Path) {
    let _ = fs::remove_file(request);
    let _ = fs::remove_file(acknowledgement);
}

fn write_gc_request(
    request: &Path,
    acknowledgement: &Path,
    protocol: &str,
    request_id: &str,
    child_pid: u32,
) -> Result<()> {
    reset_gc_channel(request, acknowledgement);
    let temporary = request.with_extension("request.tmp");
    let payload = json!({
        "schema_id": protocol,
        "request_id": request_id,
        "action": "full-gc",
        "child_pid": child_pid,
    });
    fs::write(
        &temporary,
        serde_json::to_vec(&payload)
            .map_err(|error| Error::runtime(format!("serialize GC request: {error}")))?,
    )
    .map_err(|error| Error::io("write cooperative GC request", error))?;
    if let Err(error) = fs::rename(&temporary, request) {
        let _ = fs::remove_file(&temporary);
        return Err(Error::io("publish cooperative GC request", error));
    }
    Ok(())
}

fn gc_acknowledged(acknowledgement: &Path, protocol: &str, request_id: &str) -> bool {
    let Ok(metadata) = fs::metadata(acknowledgement) else {
        return false;
    };
    if metadata.len() > 16 * 1024 {
        return false;
    }
    let Ok(contents) = fs::read(acknowledgement) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&contents) else {
        return false;
    };
    value.get("schema_id").and_then(|value| value.as_str()) == Some(protocol)
        && value.get("request_id").and_then(|value| value.as_str()) == Some(request_id)
        && value.get("action").and_then(|value| value.as_str()) == Some("full-gc")
        && value.get("status").and_then(|value| value.as_str()) == Some("completed")
}

pub fn run(repository: &Path, policy: &Policy, mut options: RunOptions) -> Result<RunOutcome> {
    if options.command.is_empty() {
        return Err(Error::usage("runtime run requires a command after --"));
    }
    if options.stdin_payload.as_ref().is_some_and(|payload| {
        payload.0.is_empty() || payload.0.len() > 4096 || options.keep_stdin_open
    }) {
        return Err(Error::usage("bounded stdin has invalid size or lifetime"));
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
    if profile.hard_containment_required && !platform_hard_containment_available()? {
        return Err(Error::runtime(
            "E_CLEARRA_HARD_CONTAINMENT_UNAVAILABLE: this profile requires a Windows Job Object or an inherited finite Linux cgroup",
        ));
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
        .stdin(
            if options.keep_stdin_open || options.stdin_payload.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            },
        )
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
            "CLEARRA_MEMORY_PRESSURE_PROTOCOL",
            &policy
                .runtime_policy
                .memory_pressure
                .cooperative_gc_protocol,
        )
        .env("CLEARRA_MEMORY_PRESSURE_REQUEST_PATH", &gc_request)
        .env("CLEARRA_MEMORY_PRESSURE_ACK_PATH", &gc_ack)
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
    let mut stdin_lease = child.stdin.take();
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

    if let Some(payload) = options.stdin_payload.take() {
        let delivery = stdin_lease.as_mut().map(|sink| sink.write_all(&payload.0));
        drop(stdin_lease.take());
        if !matches!(delivery, Some(Ok(()))) {
            terminate_tree(&mut child, &containment, profile.termination_grace_seconds);
            let _ = child.wait();
            return Err(Error::runtime("bounded stdin delivery failed"));
        }
    } else if !options.keep_stdin_open {
        drop(stdin_lease.take());
    }

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
    // Observe once immediately after the child enters containment. Start
    // admission only protects the smaller critical reserve; a non-critical
    // pressure state must therefore request GC without waiting a full sample.
    let mut next_sample = Instant::now();
    let mut pressure_seen = false;
    let mut pressure_events = 0u64;
    let mut pressure_recoveries = 0u64;
    let mut gc_requests = 0u64;
    let mut gc_acknowledgements = 0u64;
    let mut pressure_checks = 0u64;
    let mut last_pressure_snapshot = None;
    let mut last_pressure_status = None;
    let mut active_gc_request: Option<String> = None;
    let mut active_gc_acknowledged = false;
    let mut recovery_deadline: Option<Instant> = None;
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
        let now = Instant::now();
        let pressure_due = recovery_deadline.is_some_and(|deadline| now >= deadline);
        if now >= next_sample || pressure_due {
            next_sample = now + sample_interval;
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
            let current = match memory_snapshot() {
                Ok(value) => value,
                Err(_) => {
                    forced_reason = Some((
                        "memory-pressure-observer",
                        "E_CLEARRA_MEMORY_PRESSURE_OBSERVER_FAILED",
                    ));
                    terminate_tree(&mut child, &containment, profile.termination_grace_seconds);
                    status = child.wait().map_err(|error| {
                        Error::io("wait after memory-pressure observer failure", error)
                    })?;
                    break;
                }
            };
            let pressure = &policy.runtime_policy.memory_pressure;
            let observation = memory_pressure_status(pressure, current, host_memory_model());
            pressure_checks += 1;
            last_pressure_snapshot = Some(current);
            last_pressure_status = Some(observation);

            if let Some(request_id) = active_gc_request.as_deref() {
                if !active_gc_acknowledged
                    && gc_acknowledged(&gc_ack, &pressure.cooperative_gc_protocol, request_id)
                {
                    active_gc_acknowledged = true;
                    gc_acknowledgements += 1;
                }
            }

            if !observation.under_pressure() {
                if active_gc_request.take().is_some() {
                    pressure_recoveries += 1;
                    active_gc_acknowledged = false;
                    recovery_deadline = None;
                    reset_gc_channel(&gc_request, &gc_ack);
                }
            } else {
                pressure_seen = true;
                if active_gc_request.is_none() {
                    pressure_events += 1;
                    let request_number = gc_requests + 1;
                    let request_id = format!("{run_id}-{request_number}");
                    if write_gc_request(
                        &gc_request,
                        &gc_ack,
                        &pressure.cooperative_gc_protocol,
                        &request_id,
                        child.id(),
                    )
                    .is_err()
                    {
                        forced_reason = Some((
                            "gc-request-failed",
                            "E_CLEARRA_MEMORY_PRESSURE_GC_REQUEST_FAILED",
                        ));
                        terminate_tree(&mut child, &containment, profile.termination_grace_seconds);
                        status = child.wait().map_err(|error| {
                            Error::io("wait after cooperative GC request failure", error)
                        })?;
                        break;
                    }
                    gc_requests = request_number;
                    active_gc_request = Some(request_id);
                    active_gc_acknowledged = false;
                    recovery_deadline = Some(
                        now + Duration::from_secs_f64(pressure.recovery_grace_seconds.max(0.0)),
                    );
                } else if recovery_deadline.is_some_and(|deadline| now >= deadline) {
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
        gc_request_written: gc_requests > 0,
        gc_acknowledged: gc_acknowledgements > 0,
        memory_pressure_events: pressure_events,
        memory_pressure_recoveries: pressure_recoveries,
        cooperative_gc_requests: gc_requests,
        cooperative_gc_acknowledgements: gc_acknowledgements,
        memory_pressure_checks: pressure_checks,
        last_memory_pressure_snapshot: last_pressure_snapshot,
        last_memory_pressure_status: last_pressure_status,
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
    reset_gc_channel(&gc_request, &gc_ack);
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

#[cfg(windows)]
fn platform_hard_containment_available() -> Result<bool> {
    Ok(true)
}

#[cfg(target_os = "linux")]
fn platform_hard_containment_available() -> Result<bool> {
    let membership = fs::read_to_string("/proc/self/cgroup")
        .map_err(|error| Error::io("read Linux cgroup membership", error))?;
    let relative = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .ok_or_else(|| Error::runtime("cgroup v2 membership is unavailable"))?;
    let root = Path::new("/sys/fs/cgroup");
    let directory = root.join(relative.trim_start_matches('/'));
    Ok(
        inherited_finite_cgroup_limit(&directory, root, "memory.max")
            && inherited_finite_cgroup_limit(&directory, root, "pids.max"),
    )
}

#[cfg(not(any(windows, target_os = "linux")))]
fn platform_hard_containment_available() -> Result<bool> {
    Ok(false)
}

#[cfg(target_os = "linux")]
fn finite_cgroup_limit(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .as_deref()
        .and_then(finite_cgroup_limit_value)
        .is_some()
}

#[cfg(target_os = "linux")]
fn inherited_finite_cgroup_limit(directory: &Path, root: &Path, name: &str) -> bool {
    let mut cursor = directory.to_path_buf();
    if !cursor.starts_with(root) {
        return false;
    }
    loop {
        if finite_cgroup_limit(&cursor.join(name)) {
            return true;
        }
        if cursor == root || !cursor.pop() || !cursor.starts_with(root) {
            return false;
        }
    }
}

#[cfg(target_os = "linux")]
fn finite_cgroup_limit_value(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok().filter(|value| *value > 0)
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

    #[test]
    fn bounded_stdin_debug_never_displays_payload() {
        let payload = BoundedStdin::new(b"test-only-marker".to_vec());
        let debug = format!("{payload:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("test-only-marker"));
    }

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

    fn pressure() -> MemoryPressurePolicy {
        MemoryPressurePolicy {
            sample_interval_seconds: 2.0,
            recovery_grace_seconds: 2.0,
            physical_reserve_mib: 512,
            physical_reserve_fraction: 0.03125,
            commit_reserve_mib: 1024,
            commit_reserve_fraction: 0.03125,
            critical_physical_reserve_mib: 128,
            critical_commit_reserve_mib: 256,
            cooperative_gc_protocol: "clearra.memory-pressure.v1".to_owned(),
            request_cooperative_full_gc: true,
            require_ack_for_child_full_gc_claim: true,
            automatic_retry: false,
        }
    }

    fn snapshot(physical_available_mib: u64, commit_available_mib: u64) -> MemorySnapshot {
        MemorySnapshot {
            physical_total_bytes: 16 * 1024 * MIB,
            physical_available_bytes: physical_available_mib * MIB,
            commit_limit_bytes: 32 * 1024 * MIB,
            commit_available_bytes: commit_available_mib * MIB,
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

    #[test]
    fn startup_does_not_reserve_the_declared_working_set() {
        let mut build = profile();
        build.minimum_memory_mib = 3072;
        build.maximum_memory_mib = Some(6144);
        let admission = calculate_admission_for_profile(
            "build-test",
            &build,
            &pressure(),
            snapshot(768, 2048),
            HostMemoryModel::WindowsCommit,
        )
        .expect("critical reserves are enough to start");

        assert_eq!(admission.minimum_bytes, 3072 * MIB);
        assert_eq!(admission.hard_limit_bytes, 6144 * MIB);
        assert_eq!(admission.start_admission_mode, "critical-reserve");
        assert_eq!(admission.capacity_basis, "runtime-pressure");
    }

    #[test]
    fn startup_rejects_only_a_missing_critical_reserve() {
        let error = calculate_admission_for_profile(
            "control",
            &profile(),
            &pressure(),
            snapshot(127, 4096),
            HostMemoryModel::WindowsCommit,
        )
        .expect_err("physical critical reserve must remain available");
        assert!(error
            .to_string()
            .contains("E_CLEARRA_MEMORY_ADMISSION_DENIED"));

        let error = calculate_admission_for_profile(
            "control",
            &profile(),
            &pressure(),
            snapshot(4096, 255),
            HostMemoryModel::WindowsCommit,
        )
        .expect_err("commit critical reserve must remain available");
        assert!(error.to_string().contains("commit-available"));
    }

    #[test]
    fn unbounded_limit_uses_stable_capacity_instead_of_start_availability() {
        let mut benchmark = profile();
        benchmark.minimum_memory_mib = 4096;
        benchmark.maximum_memory_mib = None;
        let windows = calculate_admission_for_profile(
            "benchmark-search",
            &benchmark,
            &pressure(),
            snapshot(768, 2048),
            HostMemoryModel::WindowsCommit,
        )
        .expect("Windows commit capacity is stable");
        assert_eq!(windows.hard_limit_bytes, 31 * 1024 * MIB);

        let physical = calculate_admission_for_profile(
            "benchmark-search",
            &benchmark,
            &pressure(),
            snapshot(768, 2048),
            HostMemoryModel::Physical,
        )
        .expect("physical capacity is stable");
        assert_eq!(physical.hard_limit_bytes, 15_872 * MIB);
    }

    #[test]
    fn pressure_reserve_honors_the_fraction_floor() {
        let large = MemorySnapshot {
            physical_total_bytes: 64 * 1024 * MIB,
            physical_available_bytes: 4096 * MIB,
            commit_limit_bytes: 64 * 1024 * MIB,
            commit_available_bytes: 4096 * MIB,
        };
        let status = memory_pressure_status(&pressure(), large, HostMemoryModel::WindowsCommit);
        assert_eq!(status.physical_reserve_bytes, 2048 * MIB);
        assert_eq!(status.commit_reserve_bytes, Some(2048 * MIB));
        assert!(!status.under_pressure());
    }

    #[test]
    fn full_gc_acknowledgement_must_match_the_request() {
        let root = state_root()
            .expect("resolve managed test state")
            .join("tests")
            .join(format!("clearra-gc-{}", run_id()));
        fs::create_dir_all(&root).expect("create fixture root");
        let request = root.join("request.json");
        let acknowledgement = root.join("ack.json");
        write_gc_request(
            &request,
            &acknowledgement,
            "clearra.memory-pressure.v1",
            "expected",
            42,
        )
        .expect("write request");
        fs::write(
            &acknowledgement,
            br#"{"schema_id":"clearra.memory-pressure.v1","request_id":"wrong","action":"full-gc","status":"completed"}"#,
        )
        .expect("write wrong acknowledgement");
        assert!(!gc_acknowledged(
            &acknowledgement,
            "clearra.memory-pressure.v1",
            "expected"
        ));
        fs::write(
            &acknowledgement,
            br#"{"schema_id":"clearra.memory-pressure.v1","request_id":"expected","action":"full-gc","status":"completed"}"#,
        )
        .expect("write matching acknowledgement");
        assert!(gc_acknowledged(
            &acknowledgement,
            "clearra.memory-pressure.v1",
            "expected"
        ));
        write_gc_request(
            &request,
            &acknowledgement,
            "clearra.memory-pressure.v1",
            "second",
            42,
        )
        .expect("rearm the pressure channel");
        assert!(!gc_acknowledged(
            &acknowledgement,
            "clearra.memory-pressure.v1",
            "second"
        ));
        reset_gc_channel(&request, &acknowledgement);
        let _ = fs::remove_dir(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn finite_cgroup_limit_rejects_unbounded_or_invalid_values() {
        assert_eq!(finite_cgroup_limit_value("max\n"), None);
        assert_eq!(finite_cgroup_limit_value("0\n"), None);
        assert_eq!(finite_cgroup_limit_value("invalid\n"), None);
        assert_eq!(
            finite_cgroup_limit_value("1073741824\n"),
            Some(1_073_741_824)
        );
    }
}
