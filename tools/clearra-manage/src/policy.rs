use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Policy {
    pub schema_id: String,
    pub policy_version: u64,
    pub managed_scope: Vec<String>,
    pub excluded_scope: Vec<String>,
    #[serde(default)]
    pub toolchains: BTreeMap<String, String>,
    #[serde(default)]
    pub toolchain_sources: BTreeMap<String, serde_json::Value>,
    pub repository_roots: Vec<RepositoryRoot>,
    pub external_roots: Vec<ExternalRoot>,
    #[serde(default)]
    pub forbidden_repository_roots: Vec<String>,
    #[serde(default)]
    pub secret_path_patterns: Vec<String>,
    pub resource_profiles: BTreeMap<String, ResourceProfile>,
    pub runtime_policy: RuntimePolicy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryRoot {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub classes: Vec<String>,
    pub lifecycle: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalRoot {
    pub id: String,
    pub windows: String,
    pub linux: String,
    pub lifecycle: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceProfile {
    pub minimum_memory_mib: u64,
    pub maximum_memory_mib: Option<u64>,
    pub timeout_seconds: u64,
    pub maximum_timeout_seconds: u64,
    pub termination_grace_seconds: u64,
    pub maximum_descendant_processes: u32,
    pub output_limit_bytes: u64,
    pub hard_containment_required: bool,
    pub oom_policy: String,
    pub admission_basis: String,
    pub start_admission: String,
    pub memory_pressure_action: String,
    pub concurrency_class: String,
    #[serde(default)]
    pub explicit_timeout_required: bool,
    #[serde(default)]
    pub explicit_lease_required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimePolicy {
    pub memory_pressure: MemoryPressurePolicy,
    pub parallel_admission: ParallelAdmissionPolicy,
    pub process_tree_contract: ProcessTreeContract,
    pub local_ports: BTreeMap<String, serde_json::Value>,
    pub wsl: WslPolicy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryPressurePolicy {
    pub sample_interval_seconds: f64,
    pub recovery_grace_seconds: f64,
    pub physical_reserve_mib: u64,
    pub physical_reserve_fraction: f64,
    pub commit_reserve_mib: u64,
    pub commit_reserve_fraction: f64,
    pub critical_physical_reserve_mib: u64,
    pub critical_commit_reserve_mib: u64,
    pub cooperative_gc_protocol: String,
    pub request_cooperative_full_gc: bool,
    pub require_ack_for_child_full_gc_claim: bool,
    pub automatic_retry: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParallelAdmissionPolicy {
    pub stale_slot_grace_seconds: u64,
    pub classes: BTreeMap<String, ParallelClass>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParallelClass {
    pub maximum_parallel: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessTreeContract {
    pub windows_job_kill_on_close: bool,
    pub windows_aggregate_memory_limit: bool,
    pub windows_active_process_limit: bool,
    pub linux_process_group: bool,
    pub wsl_terminate_distribution: String,
    pub wsl_global_shutdown_allowed: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WslPolicy {
    pub distribution: String,
    pub source_distribution: String,
    pub marker: String,
    pub allow_shutdown: bool,
    pub terminate_only_dedicated_distribution: bool,
    pub entrypoints: BTreeMap<String, WslEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WslEntry {
    pub profile: String,
    #[serde(default)]
    pub requires_source: bool,
    /// Explicit, repository-local working-copy files required by a local-only
    /// generator before they are tracked. They never grant release authority.
    #[serde(default)]
    pub local_source_files: Vec<String>,
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub output_path_options: Vec<String>,
    #[serde(default)]
    pub input_path_options: Vec<String>,
}

impl Policy {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("config/clearra-management.v1.json");
        let bytes = fs::read(&path)
            .map_err(|error| Error::policy(format!("cannot read {}: {error}", path.display())))?;
        let policy: Self = serde_json::from_slice(&bytes)
            .map_err(|error| Error::policy(format!("cannot parse {}: {error}", path.display())))?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_id != "clearra.management-policy.v2" || self.policy_version != 2 {
            return Err(Error::policy("unsupported management policy schema"));
        }
        let managed = self
            .managed_scope
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let expected_managed = HashSet::from([
            "generated-output-paths",
            "process-memory",
            "process-tree",
            "wsl-lifecycle",
        ]);
        if managed != expected_managed {
            return Err(Error::policy(
                "managed scope must stay limited to output paths and runtime safety",
            ));
        }
        let excluded = self
            .excluded_scope
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        for domain in [
            "filesystem-reading",
            "git",
            "dependency-management",
            "toolchain-installation",
            "package-publishing",
            "general-process-registration",
        ] {
            if !excluded.contains(domain) {
                return Err(Error::policy(format!(
                    "manager scope exclusion is missing: {domain}"
                )));
            }
        }
        if self.toolchains.is_empty() || self.toolchain_sources.is_empty() {
            return Err(Error::policy("WSL toolchain identity is incomplete"));
        }
        let mut ids = HashSet::new();
        for root in &self.repository_roots {
            validate_relative_root(&root.path)?;
            if root.id.trim().is_empty() || !ids.insert(format!("repo:{}", root.id)) {
                return Err(Error::policy(
                    "repository root IDs must be non-empty and unique",
                ));
            }
            if root.lifecycle.trim().is_empty() || root.classes.is_empty() {
                return Err(Error::policy(format!(
                    "repository root {} lacks lifecycle/classes",
                    root.id
                )));
            }
        }
        for root in &self.external_roots {
            if root.id.trim().is_empty() || !ids.insert(format!("external:{}", root.id)) {
                return Err(Error::policy(
                    "external root IDs must be non-empty and unique",
                ));
            }
            if root.windows.trim().is_empty()
                || root.linux.trim().is_empty()
                || root.lifecycle.trim().is_empty()
            {
                return Err(Error::policy(format!(
                    "external root {} is incomplete",
                    root.id
                )));
            }
        }
        for (name, profile) in &self.resource_profiles {
            if profile.minimum_memory_mib == 0
                || profile
                    .maximum_memory_mib
                    .is_some_and(|maximum| maximum < profile.minimum_memory_mib)
                || profile.timeout_seconds == 0
                || profile.timeout_seconds > profile.maximum_timeout_seconds
                || profile.maximum_descendant_processes == 0
                || profile.output_limit_bytes == 0
                || profile.oom_policy != "fail-no-retry"
                || profile.admission_basis != "runtime-pressure"
                || profile.start_admission != "critical-reserve"
                || profile.memory_pressure_action != "gc-then-fail-close"
            {
                return Err(Error::policy(format!("invalid resource profile: {name}")));
            }
            let class = self
                .runtime_policy
                .parallel_admission
                .classes
                .get(&profile.concurrency_class)
                .ok_or_else(|| Error::policy(format!("unknown concurrency class for {name}")))?;
            if class.maximum_parallel == 0 {
                return Err(Error::policy(format!("zero concurrency for {name}")));
            }
        }
        let tree = &self.runtime_policy.process_tree_contract;
        if !tree.windows_job_kill_on_close
            || !tree.windows_aggregate_memory_limit
            || !tree.windows_active_process_limit
            || !tree.linux_process_group
            || tree.wsl_global_shutdown_allowed
        {
            return Err(Error::policy("process-tree safety contract is weakened"));
        }
        let wsl = &self.runtime_policy.wsl;
        if wsl.distribution != tree.wsl_terminate_distribution
            || wsl.allow_shutdown
            || !wsl.terminate_only_dedicated_distribution
        {
            return Err(Error::policy("WSL ownership contract is inconsistent"));
        }
        if self.runtime_policy.memory_pressure.automatic_retry {
            return Err(Error::policy("automatic OOM retry must remain disabled"));
        }
        let pressure = &self.runtime_policy.memory_pressure;
        if pressure.sample_interval_seconds <= 0.0
            || !pressure.sample_interval_seconds.is_finite()
            || pressure.recovery_grace_seconds < 0.0
            || !pressure.recovery_grace_seconds.is_finite()
            || !(0.0..=1.0).contains(&pressure.physical_reserve_fraction)
            || !(0.0..=1.0).contains(&pressure.commit_reserve_fraction)
            || pressure.physical_reserve_mib < pressure.critical_physical_reserve_mib
            || pressure.commit_reserve_mib < pressure.critical_commit_reserve_mib
        {
            return Err(Error::policy(
                "invalid memory-pressure timing or reserve fraction",
            ));
        }
        if !pressure.request_cooperative_full_gc
            || !self
                .runtime_policy
                .memory_pressure
                .require_ack_for_child_full_gc_claim
            || self.runtime_policy.memory_pressure.cooperative_gc_protocol
                != "clearra.memory-pressure.v1"
        {
            return Err(Error::policy(
                "cooperative GC accounting contract is weakened",
            ));
        }
        if self
            .resource_profiles
            .get("cloud-job")
            .is_none_or(|profile| !profile.hard_containment_required)
        {
            return Err(Error::policy("cloud-job must require hard containment"));
        }
        if self
            .runtime_policy
            .parallel_admission
            .stale_slot_grace_seconds
            == 0
        {
            return Err(Error::policy(
                "runtime slot stale-owner grace must be positive",
            ));
        }
        for port in ["4194", "4195", "8790"] {
            if !self.runtime_policy.local_ports.contains_key(port) {
                return Err(Error::policy(format!(
                    "reserved local port is missing: {port}"
                )));
            }
        }
        if wsl.source_distribution == wsl.distribution || wsl.marker != "/etc/clearra/runtime.json"
        {
            return Err(Error::policy("WSL source/marker contract is invalid"));
        }
        for (name, entry) in &wsl.entrypoints {
            if !entry.requires_source && !entry.local_source_files.is_empty() {
                return Err(Error::policy(format!(
                    "WSL entry {name} declares local files without a source archive"
                )));
            }
            if !entry.local_source_files.is_empty()
                && (entry.profile != "benchmark-search"
                    || !matches!(
                        name.as_str(),
                        "legal-board-generate"
                            | "conditioned-reachability-generate"
                            | "conditioned-local-relation-generate"
                    ))
            {
                return Err(Error::policy(format!(
                    "WSL entry {name} cannot include working-copy-only source"
                )));
            }
            let allowed = match name.as_str() {
                "legal-board-generate" => &[
                    "crates/clearra-accelerator-runtime/Cargo.toml",
                    "crates/clearra-accelerator-runtime/src/lib.rs",
                    "crates/clearra-core-executor/src/backend/wasm_cpu/reachability_local_relation.rs",
                    "crates/clearra-core-executor/src/backend/wasm_cpu/reachability_reference_tests.rs",
                    "crates/clearra-core-executor/src/conditioned_local_index.rs",
                    "crates/clearra-core-executor/src/conditioned_local_pack.rs",
                    "crates/clearra-core-executor/src/conditioned_local_pack_tests.rs",
                    "crates/clearra-core-executor/src/conditioned_local_qualification.rs",
                    "crates/clearra-core-executor/src/conditioned_local_relation.rs",
                    "crates/clearra-core-executor/src/reachability_reference.rs",
                    "crates/clearra-core-executor/src/reachability_reference_local.rs",
                ][..],
                "conditioned-reachability-generate" => &[
                    "crates/clearra-accelerator-runtime/Cargo.toml",
                    "crates/clearra-accelerator-runtime/src/lib.rs",
                    "scripts/tools/wsl-conditioned-reachability-generate.sh",
                    "crates/clearra-core-executor/src/backend/wasm_cpu/reachability_local_relation.rs",
                    "crates/clearra-core-executor/src/backend/wasm_cpu/reachability_reference_tests.rs",
                    "crates/clearra-core-executor/src/conditioned_local_index.rs",
                    "crates/clearra-core-executor/src/conditioned_local_pack.rs",
                    "crates/clearra-core-executor/src/conditioned_local_pack_tests.rs",
                    "crates/clearra-core-executor/src/conditioned_local_qualification.rs",
                    "crates/clearra-core-executor/src/conditioned_local_relation.rs",
                    "crates/clearra-core-executor/src/reachability_reference.rs",
                    "crates/clearra-core-executor/src/reachability_reference_local.rs",
                ][..],
                "conditioned-local-relation-generate" => &[
                    "crates/clearra-accelerator-runtime/Cargo.toml",
                    "crates/clearra-accelerator-runtime/src/lib.rs",
                    "scripts/tools/wsl-conditioned-local-relation-generate.sh",
                    "tools/clearra-pc4-qualifier/src/bin/clearra-conditioned-local-relation.rs",
                    "tools/clearra-pc4-qualifier/src/conditioned_local_relation_generation.rs",
                    "crates/clearra-core-executor/src/backend/wasm_cpu/reachability_local_relation.rs",
                    "crates/clearra-core-executor/src/backend/wasm_cpu/reachability_reference_tests.rs",
                    "crates/clearra-core-executor/src/conditioned_local_index.rs",
                    "crates/clearra-core-executor/src/conditioned_local_pack.rs",
                    "crates/clearra-core-executor/src/conditioned_local_pack_tests.rs",
                    "crates/clearra-core-executor/src/conditioned_local_qualification.rs",
                    "crates/clearra-core-executor/src/conditioned_local_relation.rs",
                    "crates/clearra-core-executor/src/reachability_reference.rs",
                    "crates/clearra-core-executor/src/reachability_reference_local.rs",
                ][..],
                _ => &[][..],
            };
            if entry.local_source_files.len() != allowed.len() {
                return Err(Error::policy(format!(
                    "WSL entry {name} has an incomplete working-copy source allowlist"
                )));
            }
            if entry
                .local_source_files
                .iter()
                .any(|path| !allowed.contains(&path.as_str()))
                || entry
                    .local_source_files
                    .iter()
                    .enumerate()
                    .any(|(index, path)| entry.local_source_files[..index].contains(path))
            {
                return Err(Error::policy(format!(
                    "WSL entry {name} has an unapproved working-copy source"
                )));
            }
        }
        Ok(())
    }

    pub fn profile(&self, name: &str) -> Result<&ResourceProfile> {
        self.resource_profiles
            .get(name)
            .ok_or_else(|| Error::usage(format!("unknown resource profile: {name}")))
    }

    pub fn external_path(&self, root: &ExternalRoot) -> Result<PathBuf> {
        let template = if cfg!(windows) {
            &root.windows
        } else {
            &root.linux
        };
        expand_template(template)
    }
}

pub fn discover_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let start = match explicit {
        Some(path) => path,
        None => env::current_dir().map_err(|error| Error::io("read current directory", error))?,
    };
    let mut cursor = if start.is_file() {
        start.parent().unwrap_or(&start).to_path_buf()
    } else {
        start
    };
    loop {
        if cursor.join("config/clearra-management.v1.json").is_file() {
            return fs::canonicalize(&cursor)
                .map_err(|error| Error::io("canonicalize repository root", error));
        }
        if !cursor.pop() {
            break;
        }
    }
    Err(Error::policy(
        "could not locate config/clearra-management.v1.json",
    ))
}

fn validate_relative_root(value: &str) -> Result<()> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() {
        return Err(Error::policy(format!(
            "repository root must be relative: {value}"
        )));
    }
    for component in path.components() {
        if matches!(
            component,
            std::path::Component::ParentDir | std::path::Component::CurDir
        ) {
            return Err(Error::policy(format!(
                "repository root is not normalized: {value}"
            )));
        }
    }
    Ok(())
}

fn expand_template(template: &str) -> Result<PathBuf> {
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or_else(|| Error::policy("HOME/USERPROFILE is unavailable"))?;
    let home = PathBuf::from(home).to_string_lossy().replace('\\', "/");
    let local_app_data = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&home).join("AppData/Local"))
        .to_string_lossy()
        .replace('\\', "/");
    let cache = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&home).join(".cache"))
        .to_string_lossy()
        .replace('\\', "/");
    let state = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&home).join(".local/state"))
        .to_string_lossy()
        .replace('\\', "/");
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .to_string_lossy()
        .replace('\\', "/");
    let expanded = template
        .replace("${XDG_CACHE_HOME:-${HOME}/.cache}", &cache)
        .replace("${XDG_STATE_HOME:-${HOME}/.local/state}", &state)
        .replace("${XDG_RUNTIME_DIR:-/tmp}", &runtime)
        .replace("${LOCALAPPDATA}", &local_app_data)
        .replace("${HOME}", &home);
    if expanded.contains("${") {
        return Err(Error::policy(format!(
            "unsupported path template: {template}"
        )));
    }
    Ok(PathBuf::from(expanded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_roots_reject_parent_traversal() {
        assert!(validate_relative_root("../target").is_err());
        assert!(validate_relative_root("build/../target").is_err());
        assert!(validate_relative_root("build/cargo").is_ok());
    }
}
