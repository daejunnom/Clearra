use serde_json::json;
use std::fs;
use std::io::IsTerminal;
use std::path::{Component, Path, PathBuf};

use crate::policy::Policy;
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
    Ok(json!({
        "schema_id": "clearra.storage-audit.v2",
        "repository": repository,
        "roots": roots,
        "violations": violations,
        "valid": violations.is_empty(),
    }))
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
    let mut cursor = PathBuf::new();
    for component in path.components() {
        cursor.push(component.as_os_str());
        if !cursor.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&cursor)
            .map_err(|error| Error::io("inspect path component", error))?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(Error::storage(format!(
                "link or junction escape is not allowed: {}",
                cursor.display()
            )));
        }
    }
    Ok(())
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
