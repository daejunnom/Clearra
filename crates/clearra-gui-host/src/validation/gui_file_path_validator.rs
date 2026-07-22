use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use crate::validation::{GuiValidationDiagnostic, GuiValidationSummary};

const MAX_JSON_INPUT_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GuiFilePathValidator;

impl GuiFilePathValidator {
    pub fn validate_fixture_file_path(
        path: impl AsRef<Path>,
        verbose_paths: bool,
    ) -> GuiValidationSummary {
        let path = path.as_ref();
        let display = Self::display_input_path(path, verbose_paths);
        let mut summary = GuiValidationSummary::new();

        if path.as_os_str().is_empty() {
            summary.push(GuiValidationDiagnostic::unsafe_file_path(
                &display,
                "file path must not be empty",
            ));
            return summary;
        }

        let file_name = match path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        {
            Some(file_name) => file_name,
            None => {
                summary.push(GuiValidationDiagnostic::unsafe_file_path(
                    &display,
                    format!("file path '{display}' must include a file name"),
                ));
                return summary;
            }
        };

        if path.components().any(secret_like_component) || is_secret_like_name(&file_name) {
            summary.push(GuiValidationDiagnostic::unsafe_file_path(
                &display,
                format!("refusing to read sensitive-looking file path '{display}'"),
            ));
            return summary;
        }

        let is_json = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
        if !is_json {
            summary.push(GuiValidationDiagnostic::unsafe_file_path(
                &display,
                format!("file path '{display}' must be a .json file"),
            ));
            return summary;
        }

        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    summary.push(GuiValidationDiagnostic::unsafe_file_path(
                        &display,
                        format!("refusing to read symlinked file path '{display}'"),
                    ));
                } else if !metadata.is_file() {
                    summary.push(GuiValidationDiagnostic::unsafe_file_path(
                        &display,
                        format!("file path '{display}' is not a file"),
                    ));
                } else if metadata.len() > MAX_JSON_INPUT_BYTES {
                    summary.push(GuiValidationDiagnostic::unsafe_file_path(
                        &display,
                        format!(
                            "file path '{display}' is too large ({} bytes, limit {MAX_JSON_INPUT_BYTES})",
                            metadata.len()
                        ),
                    ));
                }
            }
            Err(error) => summary.push(GuiValidationDiagnostic::unsafe_file_path(
                &display,
                format!("failed to inspect file path '{display}': {error}"),
            )),
        }

        summary
    }
}
impl GuiFilePathValidator {
    pub fn display_input_path(path: impl AsRef<Path>, verbose_paths: bool) -> String {
        let path = path.as_ref();
        if verbose_paths {
            return path.display().to_string();
        }
        redacted_path(path)
    }
}

fn secret_like_component(component: Component<'_>) -> bool {
    match component {
        Component::Normal(name) => is_secret_like_name(&name.to_string_lossy()),
        _ => false,
    }
}

fn is_secret_like_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".env"
        || lower.starts_with(".env.")
        || lower == "id_rsa"
        || lower == "id_dsa"
        || lower == "id_ecdsa"
        || lower == "id_ed25519"
        || lower.contains("service-account")
        || lower.contains("service_account")
        || lower.contains("credential")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("secret")
}

fn redacted_path(path: &Path) -> String {
    if path.is_relative() {
        return normalize_display(path);
    }

    if let Ok(current_dir) = std::env::current_dir() {
        if let Ok(relative) = path.strip_prefix(current_dir) {
            return normalize_display(relative);
        }
    }

    path.file_name()
        .map(|name| PathBuf::from("...").join(name))
        .map(|path| normalize_display(&path))
        .unwrap_or_else(|| "<redacted-path>".to_owned())
}

fn normalize_display(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}
