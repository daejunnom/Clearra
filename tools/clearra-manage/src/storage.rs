use serde_json::{json, Value};
use std::fs;
use std::io::IsTerminal;
use std::path::{Component, Path, PathBuf};

use crate::policy::{ExternalInventoryRoot, Policy};
use crate::{Error, Result};

pub const PATH_ERROR: &str = "E_CLEARRA_STORAGE_PATH_NOT_ALLOWED";

#[derive(Debug, Clone)]
pub struct VerifiedPath {
    pub path: PathBuf,
    pub root_id: String,
    pub lifecycle: String,
}

pub fn verify(
    repository: &Path,
    policy: &Policy,
    requested: &Path,
    force: bool,
    reason: Option<&str>,
) -> Result<VerifiedPath> {
    if is_secret_path(policy, requested) {
        return Err(Error::storage(
            "prohibited credential path blocked; contents were not inspected",
        ));
    }
    let absolute = absolute_normalized(repository, requested)?;
    reject_existing_links(&absolute)?;

    for root in &policy.repository_roots {
        let allowed = absolute_normalized(repository, Path::new(&root.path))?;
        if path_within(&absolute, &allowed) {
            return Ok(VerifiedPath {
                path: absolute,
                root_id: root.id.clone(),
                lifecycle: root.lifecycle.clone(),
            });
        }
    }
    for root in &policy.external_roots {
        let allowed = absolute_normalized(repository, &policy.external_path(root)?)?;
        if path_within(&absolute, &allowed) {
            return Ok(VerifiedPath {
                path: absolute,
                root_id: root.id.clone(),
                lifecycle: root.lifecycle.clone(),
            });
        }
    }

    if force {
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            return Err(Error::storage(
                "unmanaged output override is available only in a local interactive terminal",
            ));
        }
        let reason = reason
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                Error::usage("--force-unmanaged-output requires --force-reason <reason>")
            })?;
        eprintln!(
            "{PATH_ERROR}: unmanaged output accepted for this invocation only; reason={reason}. 강제 실행은 권장하지 않습니다."
        );
        return Ok(VerifiedPath {
            path: absolute,
            root_id: "one-shot-unmanaged-output".to_owned(),
            lifecycle: "caller-owned".to_owned(),
        });
    }

    Err(Error::storage(format!(
        "이 위치는 Clearra 생성물 관리 정책에 포함되지 않습니다: {}\n로컬 대화형 실행에서는 --force-unmanaged-output --force-reason <이유>로 이번 실행만 강제할 수 있습니다.\n강제 실행은 권장하지 않습니다.",
        absolute.display()
    )))
}

pub fn audit(repository: &Path, policy: &Policy) -> Result<serde_json::Value> {
    let mut violations = Vec::new();
    let mut roots = Vec::new();
    for root in &policy.repository_roots {
        let path = absolute_normalized(repository, Path::new(&root.path))?;
        if path.exists() {
            if let Err(error) = reject_existing_links(&path) {
                violations.push(error.to_string());
            }
        }
        roots.push(json!({
            "id": root.id,
            "path": path,
            "classes": root.classes,
            "lifecycle": root.lifecycle,
            "exists": path.exists(),
        }));
    }
    for name in &policy.forbidden_repository_roots {
        let path = repository.join(name);
        if path.exists() {
            violations.push(format!(
                "forbidden repository artifact root exists: {}",
                path.display()
            ));
        }
    }
    let external_paths = policy
        .external_roots
        .iter()
        .map(|root| {
            (
                root,
                audit_path(repository, policy, policy.external_path(root)),
            )
        })
        .collect::<Vec<_>>();
    let external_roots = external_paths
        .iter()
        .map(|(root, path)| {
            let status = match path.as_deref().map(path_metadata) {
                Some(Ok(Some(metadata))) if metadata.is_dir() => "present",
                Some(Ok(None)) => "missing",
                _ => "unsafe-or-unavailable",
            };
            json!({"id": root.id, "path": path, "lifecycle": root.lifecycle, "status": status})
        })
        .collect::<Vec<_>>();
    let managed_external_paths = external_paths
        .iter()
        .filter_map(|(_, allowed)| allowed.as_deref())
        .collect::<Vec<_>>();
    let external_inventory = policy
        .external_inventory_roots
        .iter()
        .filter(|root| root.applies_here())
        .map(|root| {
            let path = audit_path(repository, policy, root.expanded_path());
            inventory_root(policy, root, path.as_deref(), &managed_external_paths)
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_id": "clearra.storage-audit.v2",
        "repository": repository,
        "roots": roots,
        "external_roots": external_roots,
        "external_inventory": external_inventory,
        "violations": violations,
        "valid": violations.is_empty(),
    }))
}

fn audit_path(repository: &Path, policy: &Policy, expanded: Result<PathBuf>) -> Option<PathBuf> {
    let path = expanded.ok()?;
    if !path.is_absolute() || is_secret_path(policy, &path) {
        return None;
    }
    absolute_normalized(repository, &path).ok()
}

// This diagnostic is intentionally neither a recursive scan nor output authority.
// Limit enumeration even when an unexpected directory is populated by another tool.
const MAX_INVENTORY_ENTRIES: usize = 4096;

fn inventory_root(
    policy: &Policy,
    root: &ExternalInventoryRoot,
    path: Option<&Path>,
    managed: &[&Path],
) -> Value {
    let mut entries = Vec::new();
    let mut skipped_entries = 0;
    let mut truncated = false;
    let mut status = "unsafe-or-unavailable";
    if let Some(path) = path {
        match path_metadata(path) {
            Ok(None) => status = "missing",
            Ok(Some(metadata)) if metadata.is_dir() => {
                if let Ok(children) = fs::read_dir(path) {
                    status = "ready";
                    for (index, child) in children.enumerate() {
                        if index == MAX_INVENTORY_ENTRIES {
                            truncated = true;
                            break;
                        }
                        let Ok(child) = child else {
                            skipped_entries += 1;
                            continue;
                        };
                        let child_path = child.path();
                        let Ok(name) = child.file_name().into_string() else {
                            skipped_entries += 1;
                            continue;
                        };
                        // Do not print or inspect the metadata of credential-like entries.
                        if is_secret_path(policy, &child_path) || name.chars().any(char::is_control)
                        {
                            skipped_entries += 1;
                            continue;
                        }
                        let Ok(metadata) = fs::symlink_metadata(&child_path) else {
                            skipped_entries += 1;
                            continue;
                        };
                        let class =
                            if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
                                "link"
                            } else if root.protected_children.iter().any(|protected| {
                                name == *protected
                                    || (cfg!(windows) && name.eq_ignore_ascii_case(protected))
                            }) {
                                "protected"
                            } else if managed
                                .iter()
                                .any(|allowed| path_within(&child_path, allowed))
                            {
                                "managed"
                            } else {
                                "unmanaged"
                            };
                        entries.push((name, class));
                    }
                }
            }
            _ => {}
        }
    }
    entries.sort_unstable();
    let entries = entries
        .into_iter()
        .map(|(name, class)| json!({"name": name, "classification": class}))
        .collect::<Vec<_>>();
    json!({"id": root.id, "path": path, "status": status, "entries": entries,
        "skipped_entries": skipped_entries, "truncated": truncated})
}

pub fn managed_roots_json(repository: &Path, policy: &Policy) -> Result<String> {
    let mut roots = serde_json::Map::new();
    for root in &policy.repository_roots {
        roots.insert(
            root.id.clone(),
            json!(absolute_normalized(repository, Path::new(&root.path))?),
        );
    }
    for root in &policy.external_roots {
        roots.insert(
            root.id.clone(),
            json!(absolute_normalized(
                repository,
                &policy.external_path(root)?
            )?),
        );
    }
    serde_json::to_string(&roots)
        .map_err(|error| Error::policy(format!("serialize roots: {error}")))
}

pub fn is_secret_path(policy: &Policy, path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        policy
            .secret_path_patterns
            .iter()
            .any(|pattern| wildcard_match(&pattern.to_ascii_lowercase(), &value))
    })
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let (mut p, mut v, mut star, mut matched) = (0usize, 0usize, None, 0usize);
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    while v < value.len() {
        if p < pattern.len() && pattern[p] == value[v] {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            matched = v;
        } else if let Some(index) = star {
            p = index + 1;
            matched += 1;
            v = matched;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn absolute_normalized(repository: &Path, path: &Path) -> Result<PathBuf> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::storage("parent-directory traversal is not allowed"));
    }
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repository.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(Error::storage("path escapes its filesystem root"));
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn path_within(path: &Path, root: &Path) -> bool {
    if cfg!(windows) {
        let path = windows_identity(path);
        let root = windows_identity(root).trim_end_matches('\\').to_owned();
        path == root || path.starts_with(&(root + "\\"))
    } else {
        path == root || path.starts_with(root)
    }
}

fn windows_identity(path: &Path) -> String {
    let value = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    if let Some(rest) = value.strip_prefix(r"\\?\unc\") {
        format!(r"\\{rest}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_owned()
    }
}

fn reject_existing_links(path: &Path) -> Result<()> {
    path_metadata(path).map(|_| ())
}

fn path_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    let mut cursor = PathBuf::new();
    let mut final_metadata = None;
    for component in path.components() {
        cursor.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let metadata = match fs::symlink_metadata(&cursor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Error::io("inspect path component", error)),
        };
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(Error::storage(format!(
                "link or junction escape is not allowed: {}",
                cursor.display()
            )));
        }
        final_metadata = Some(metadata);
    }
    Ok(final_metadata)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::ExternalRoot;

    #[test]
    fn external_audit_is_shallow_non_authoritative_and_tolerates_missing_roots() {
        let mut policy: Policy =
            serde_json::from_str(include_str!("../../../config/clearra-management.v1.json"))
                .expect("checked-in policy parses");
        let state = policy
            .external_roots
            .iter()
            .find(|root| root.id == "state")
            .unwrap();
        let base = policy.external_path(state).unwrap().join("tests");
        reject_existing_links(&base).unwrap();
        fs::create_dir_all(&base).unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let fixture = base.join(format!("storage-inventory-{}-{nonce}", std::process::id()));
        fs::create_dir(&fixture).unwrap();
        let inventory = fixture.join("Clearra");
        fs::create_dir(&inventory).unwrap();
        let children = [
            "build",
            "old-output",
            "worktrees",
            "redacted-fixture-sample",
        ];
        for child in children {
            fs::create_dir(inventory.join(child)).unwrap();
        }
        let nested_path = inventory.join("old-output").join("other");
        fs::create_dir(&nested_path).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(fixture.join("nonexistent"), inventory.join("broken-link"))
            .unwrap();
        let active_path = inventory.join("build").to_string_lossy().replace('\\', "/");
        let planned_missing = fixture
            .join("gone")
            .join("Clearra")
            .to_string_lossy()
            .replace('\\', "/");
        policy
            .secret_path_patterns
            .push("redacted-fixture-*".to_owned());
        policy.external_roots = vec![ExternalRoot {
            id: "fixture-owned".to_owned(),
            windows: active_path.clone(),
            linux: active_path,
            lifecycle: "fixture-retained".to_owned(),
        }];
        policy.external_inventory_roots = vec![
            ExternalInventoryRoot {
                id: "fixture-inventory".to_owned(),
                platform: std::env::consts::OS.to_owned(),
                path: inventory.to_string_lossy().replace('\\', "/"),
                protected_children: vec!["worktrees".to_owned()],
            },
            ExternalInventoryRoot {
                id: "missing-inventory".to_owned(),
                platform: std::env::consts::OS.to_owned(),
                path: planned_missing,
                protected_children: vec![],
            },
        ];
        let report = audit(&fixture, &policy).unwrap();
        // Only remove owned, explicitly named empty fixture entries; never recurse.
        #[cfg(unix)]
        fs::remove_file(inventory.join("broken-link")).unwrap();
        fs::remove_dir(nested_path).unwrap();
        for child in children {
            fs::remove_dir(inventory.join(child)).unwrap();
        }
        fs::remove_dir(&inventory).unwrap();
        fs::remove_dir(&fixture).unwrap();

        if !matches!(std::env::consts::OS, "windows" | "linux") {
            assert_eq!(report["external_inventory"], json!([]));
            return;
        }
        assert_eq!(report["valid"], true);
        assert_eq!(report["external_roots"][0]["lifecycle"], "fixture-retained");
        assert_eq!(report["external_roots"][0]["status"], "present");
        let report_root = &report["external_inventory"][0];
        assert_eq!(report_root["status"], "ready");
        assert_eq!(report_root["skipped_entries"], 1);
        let expected = [
            #[cfg(unix)]
            json!({"name":"broken-link", "classification":"link"}),
            json!({"name":"build", "classification":"managed"}),
            json!({"name":"old-output", "classification":"unmanaged"}),
            json!({"name":"worktrees", "classification":"protected"}),
        ];
        assert_eq!(report_root["entries"], json!(expected));
        assert_eq!(report["external_inventory"][1]["status"], "missing");
        assert!(verify(
            &fixture,
            &policy,
            &inventory.join("worktrees/asset"),
            false,
            None
        )
        .is_err());
    }

    #[test]
    fn wildcard_contract_matches_secret_names() {
        assert!(wildcard_match(".env.*", ".env.local"));
        assert!(wildcard_match("*.key", "deploy.key"));
        assert!(!wildcard_match("*.key", "keyboard.txt"));
    }

    #[test]
    fn path_boundary_is_component_aware() {
        let root = Path::new(if cfg!(windows) {
            r"C:\repo\build"
        } else {
            "/repo/build"
        });
        let child = Path::new(if cfg!(windows) {
            r"C:\repo\build\cargo"
        } else {
            "/repo/build/cargo"
        });
        let sibling = Path::new(if cfg!(windows) {
            r"C:\repo\builder"
        } else {
            "/repo/builder"
        });
        assert!(path_within(child, root));
        assert!(!path_within(sibling, root));
    }

    #[test]
    fn extended_windows_prefix_has_the_same_identity() {
        if cfg!(windows) {
            assert!(path_within(
                Path::new(r"C:\repo\build\cargo"),
                Path::new(r"\\?\C:\repo\build")
            ));
        }
    }

    #[test]
    fn parent_directory_syntax_is_rejected_before_normalization() {
        let repository = Path::new(if cfg!(windows) { r"C:\repo" } else { "/repo" });
        assert!(absolute_normalized(repository, Path::new("build/../build/output")).is_err());
    }
}
