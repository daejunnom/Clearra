use crate::{WebCommandError, WebCommandErrorCode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebVirtualFileHandle {
    handle_id: String,
    display_name: String,
    mime_type: String,
    byte_len: usize,
    origin_kind: &'static str,
}

impl WebVirtualFileHandle {
    pub fn new(
        handle_id: impl Into<String>,
        display_name: impl Into<String>,
        mime_type: impl Into<String>,
        byte_len: usize,
    ) -> Result<Self, WebCommandError> {
        let handle_id = handle_id.into();
        let display_name = display_name.into();
        reject_native_path_semantics(&display_name)?;
        if handle_id.trim().is_empty() || display_name.trim().is_empty() {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "browser virtual file handle requires id and display name",
            ));
        }

        Ok(Self {
            handle_id,
            display_name,
            mime_type: mime_type.into(),
            byte_len,
            origin_kind: "browser-file-input",
        })
    }
}
impl WebVirtualFileHandle {
    pub fn handle_id(&self) -> &str {
        &self.handle_id
    }
}
impl WebVirtualFileHandle {
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}
impl WebVirtualFileHandle {
    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }
}
impl WebVirtualFileHandle {
    pub const fn byte_len(&self) -> usize {
        self.byte_len
    }
}
impl WebVirtualFileHandle {
    pub const fn origin_kind(&self) -> &'static str {
        self.origin_kind
    }
}

pub(crate) fn reject_native_path_semantics(value: &str) -> Result<(), WebCommandError> {
    let trimmed = value.trim();
    let looks_like_windows_drive = trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':';
    if trimmed.starts_with('/')
        || trimmed.starts_with('~')
        || trimmed.contains('\\')
        || trimmed.contains("../")
        || trimmed.contains("..\\")
        || looks_like_windows_drive
    {
        return Err(WebCommandError::new(
            WebCommandErrorCode::NativePathSemantics,
            "browser runtime uses virtual file handles, not native paths",
        ));
    }
    Ok(())
}
