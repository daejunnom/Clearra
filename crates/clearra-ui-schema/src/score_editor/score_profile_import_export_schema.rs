use clearra_scoring::{export::ScoreProfileExport, import::ScoreProfileImport};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreProfileImportExportSchema {
    import_json_enabled: bool,
    export_json_enabled: bool,
    import_adapter: &'static str,
    export_adapter: &'static str,
}

impl ScoreProfileImportExportSchema {
    pub fn json_supported() -> Self {
        Self {
            import_json_enabled: true,
            export_json_enabled: true,
            import_adapter: "clearra-scoring::ScoreProfileImport",
            export_adapter: "clearra-scoring::ScoreProfileExport",
        }
    }
}
impl ScoreProfileImportExportSchema {
    pub fn import_json_enabled(&self) -> bool {
        self.import_json_enabled
    }
}
impl ScoreProfileImportExportSchema {
    pub fn export_json_enabled(&self) -> bool {
        self.export_json_enabled
    }
}
impl ScoreProfileImportExportSchema {
    pub fn import_adapter(&self) -> &'static str {
        self.import_adapter
    }
}
impl ScoreProfileImportExportSchema {
    pub fn export_adapter(&self) -> &'static str {
        self.export_adapter
    }
}

pub fn score_profile_json_import_adapter_marker() -> ScoreProfileImport {
    ScoreProfileImport
}

pub fn score_profile_json_export_adapter_marker() -> ScoreProfileExport {
    ScoreProfileExport
}
