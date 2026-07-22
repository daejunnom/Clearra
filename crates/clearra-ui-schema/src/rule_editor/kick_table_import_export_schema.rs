use clearra_rules::kicks::KickImport;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickTableImportExportSchema {
    import_json_enabled: bool,
    export_json_enabled: bool,
    adapter: &'static str,
}

impl KickTableImportExportSchema {
    pub fn json_supported() -> Self {
        Self {
            import_json_enabled: true,
            export_json_enabled: true,
            adapter: "clearra-rules::KickImport",
        }
    }
}
impl KickTableImportExportSchema {
    pub fn import_json_enabled(&self) -> bool {
        self.import_json_enabled
    }
}
impl KickTableImportExportSchema {
    pub fn export_json_enabled(&self) -> bool {
        self.export_json_enabled
    }
}
impl KickTableImportExportSchema {
    pub fn adapter(&self) -> &'static str {
        self.adapter
    }
}

pub fn kick_table_json_adapter_marker() -> KickImport {
    KickImport
}
