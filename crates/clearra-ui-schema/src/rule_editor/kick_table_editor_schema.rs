use clearra_rules::kicks::KickProfileRegistry;

use crate::disabled_reason::UiDisabledReason;

pub use super::{
    kick_table_import_export_schema::{
        kick_table_json_adapter_marker, KickTableImportExportSchema,
    },
    kick_table_preview_schema::KickTablePreviewSchema,
    kick_table_verification_schema::KickTableVerificationSchema,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickTableEditorSchema {
    editable: bool,
    disabled_reason: Option<UiDisabledReason>,
    import_export: KickTableImportExportSchema,
    previews: Vec<KickTablePreviewSchema>,
}

impl KickTableEditorSchema {
    pub fn unsupported_mvp() -> Self {
        Self::mvp2()
    }
}
impl KickTableEditorSchema {
    pub fn mvp2() -> Self {
        Self {
            editable: true,
            disabled_reason: None,
            import_export: KickTableImportExportSchema::json_supported(),
            previews: KickProfileRegistry::builtin_profiles()
                .into_iter()
                .map(KickTablePreviewSchema::from_descriptor)
                .collect(),
        }
    }
}
impl KickTableEditorSchema {
    pub fn editable(&self) -> bool {
        self.editable
    }
}
impl KickTableEditorSchema {
    pub fn reason(&self) -> Option<&str> {
        self.disabled_reason.as_ref().map(UiDisabledReason::reason)
    }
}
impl KickTableEditorSchema {
    pub fn disabled_reason(&self) -> Option<&UiDisabledReason> {
        self.disabled_reason.as_ref()
    }
}
impl KickTableEditorSchema {
    pub fn import_export(&self) -> &KickTableImportExportSchema {
        &self.import_export
    }
}
impl KickTableEditorSchema {
    pub fn previews(&self) -> &[KickTablePreviewSchema] {
        &self.previews
    }
}

impl Default for KickTableEditorSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}

#[cfg(test)]
#[path = "kick_table_editor_schema_tests.rs"]
mod tests;
