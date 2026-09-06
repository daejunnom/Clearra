use std::{
    cell::Cell,
    fmt, fs,
    path::{Component, Path, PathBuf},
};

const MAX_JSON_INPUT_BYTES: u64 = 1024 * 1024;
const MAX_TYPED_DOCUMENT_INPUT_BYTES: u64 = 16 * 1024 * 1024;

thread_local! {
    static VERBOSE_PATHS: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn read_json_file(path: impl AsRef<Path>) -> Result<String, FileInputError> {
    let path = path.as_ref();
    validate_json_file_path(path)?;
    fs::read_to_string(path).map_err(|error| FileInputError::Read {
        path: display_input_path(path),
        reason: error.to_string(),
    })
}

/// Reads one native typed-document argument without following link-like files.
///
/// Unlike JSON fixtures, document files deliberately have no extension
/// authority: their canonical `ctk3*`/`v115@` prefix is the only format
/// authority at the command boundary.
pub(crate) fn read_typed_document_file(path: impl AsRef<Path>) -> Result<String, FileInputError> {
    let path = path.as_ref();
    validate_typed_document_file_path(path)?;
    fs::read_to_string(path).map_err(|error| FileInputError::Read {
        path: display_input_path(path),
        reason: error.to_string(),
    })
}

pub(crate) fn with_verbose_paths<T>(verbose: bool, action: impl FnOnce() -> T) -> T {
    VERBOSE_PATHS.with(|flag| {
        let previous = flag.replace(verbose);
        let result = action();
        flag.set(previous);
        result
    })
}

pub(crate) fn display_input_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    if VERBOSE_PATHS.with(Cell::get) {
        return path.display().to_string();
    }
    redacted_path(path)
}

fn validate_json_file_path(path: &Path) -> Result<(), FileInputError> {
    let display = display_input_path(path);
    if path.as_os_str().is_empty() {
        return Err(FileInputError::EmptyPath);
    }

    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| FileInputError::MissingFileName {
            path: display.clone(),
        })?;
    if path.components().any(secret_like_component) || is_secret_like_name(&file_name) {
        return Err(FileInputError::SensitivePath { path: display });
    }

    let is_json = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    if !is_json {
        return Err(FileInputError::UnsupportedExtension { path: display });
    }

    let metadata = fs::symlink_metadata(path).map_err(|error| FileInputError::Metadata {
        path: display.clone(),
        reason: error.to_string(),
    })?;
    if metadata.file_type().is_symlink() {
        return Err(FileInputError::Symlink { path: display });
    }
    if !metadata.is_file() {
        return Err(FileInputError::NotFile { path: display });
    }
    if metadata.len() > MAX_JSON_INPUT_BYTES {
        return Err(FileInputError::TooLarge {
            path: display,
            bytes: metadata.len(),
            limit: MAX_JSON_INPUT_BYTES,
        });
    }

    Ok(())
}

fn validate_typed_document_file_path(path: &Path) -> Result<(), FileInputError> {
    let display = display_input_path(path);
    if path.as_os_str().is_empty() {
        return Err(FileInputError::EmptyPath);
    }

    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| FileInputError::MissingFileName {
            path: display.clone(),
        })?;
    if path.components().any(secret_like_component) || is_secret_like_name(&file_name) {
        return Err(FileInputError::SensitivePath { path: display });
    }

    let metadata = fs::symlink_metadata(path).map_err(|error| FileInputError::Metadata {
        path: display.clone(),
        reason: error.to_string(),
    })?;
    if link_like_file(&metadata) {
        return Err(FileInputError::Symlink { path: display });
    }
    if !metadata.is_file() {
        return Err(FileInputError::NotFile { path: display });
    }
    if metadata.len() > MAX_TYPED_DOCUMENT_INPUT_BYTES {
        return Err(FileInputError::TooLarge {
            path: display,
            bytes: metadata.len(),
            limit: MAX_TYPED_DOCUMENT_INPUT_BYTES,
        });
    }
    Ok(())
}

fn link_like_file(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    false
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
        return path.display().to_string();
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FileInputError {
    EmptyPath,
    MissingFileName {
        path: String,
    },
    SensitivePath {
        path: String,
    },
    UnsupportedExtension {
        path: String,
    },
    Metadata {
        path: String,
        reason: String,
    },
    Symlink {
        path: String,
    },
    NotFile {
        path: String,
    },
    TooLarge {
        path: String,
        bytes: u64,
        limit: u64,
    },
    Read {
        path: String,
        reason: String,
    },
}

impl fmt::Display for FileInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath => write!(formatter, "file path must not be empty"),
            Self::MissingFileName { path } => {
                write!(formatter, "file path '{path}' must include a file name")
            }
            Self::SensitivePath { path } => {
                write!(
                    formatter,
                    "refusing to read sensitive-looking file path '{path}'"
                )
            }
            Self::UnsupportedExtension { path } => {
                write!(formatter, "file path '{path}' must be a .json file")
            }
            Self::Metadata { path, reason } => {
                write!(formatter, "failed to inspect file path '{path}': {reason}")
            }
            Self::Symlink { path } => {
                write!(formatter, "refusing to read symlinked file path '{path}'")
            }
            Self::NotFile { path } => write!(formatter, "file path '{path}' is not a file"),
            Self::TooLarge { path, bytes, limit } => write!(
                formatter,
                "file path '{path}' is too large ({bytes} bytes, limit {limit})"
            ),
            Self::Read { path, reason } => write!(formatter, "failed to read '{path}': {reason}"),
        }
    }
}

#[cfg(test)]
#[path = "file_input_guard_tests.rs"]
mod tests;
