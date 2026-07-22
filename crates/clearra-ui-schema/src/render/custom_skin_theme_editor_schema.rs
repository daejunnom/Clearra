#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSkinThemeEditorSchema {
    schema_version: u32,
    editor_enabled: bool,
    runtime_preview_source: &'static str,
    raw_svg_runtime_renderer_allowed: bool,
    user_imported_asset_locations: Vec<&'static str>,
    repository_assets_allowed_for_user_imports: bool,
    manifest_and_provenance_required: bool,
    fields: Vec<CustomSkinThemeEditorFieldSchema>,
}

impl CustomSkinThemeEditorSchema {
    pub fn v1() -> Self {
        Self {
            schema_version: 1,
            editor_enabled: true,
            runtime_preview_source: "png-atlas",
            raw_svg_runtime_renderer_allowed: false,
            user_imported_asset_locations: vec!["user_config_directory", "user_cache_directory"],
            repository_assets_allowed_for_user_imports: false,
            manifest_and_provenance_required: true,
            fields: vec![
                CustomSkinThemeEditorFieldSchema::required("skin_id"),
                CustomSkinThemeEditorFieldSchema::required("palette_id"),
                CustomSkinThemeEditorFieldSchema::required("piece_mapping"),
                CustomSkinThemeEditorFieldSchema::required("grid_style"),
                CustomSkinThemeEditorFieldSchema::required("background"),
                CustomSkinThemeEditorFieldSchema::required("line_clear_highlight"),
                CustomSkinThemeEditorFieldSchema::required("ownership_color_mode"),
                CustomSkinThemeEditorFieldSchema::required("export_limits"),
                CustomSkinThemeEditorFieldSchema::required("provenance"),
            ],
        }
    }
}
impl CustomSkinThemeEditorSchema {
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
impl CustomSkinThemeEditorSchema {
    pub const fn editor_enabled(&self) -> bool {
        self.editor_enabled
    }
}
impl CustomSkinThemeEditorSchema {
    pub const fn runtime_preview_source(&self) -> &'static str {
        self.runtime_preview_source
    }
}
impl CustomSkinThemeEditorSchema {
    pub const fn raw_svg_runtime_renderer_allowed(&self) -> bool {
        self.raw_svg_runtime_renderer_allowed
    }
}
impl CustomSkinThemeEditorSchema {
    pub fn user_imported_asset_locations(&self) -> &[&'static str] {
        &self.user_imported_asset_locations
    }
}
impl CustomSkinThemeEditorSchema {
    pub const fn repository_assets_allowed_for_user_imports(&self) -> bool {
        self.repository_assets_allowed_for_user_imports
    }
}
impl CustomSkinThemeEditorSchema {
    pub const fn manifest_and_provenance_required(&self) -> bool {
        self.manifest_and_provenance_required
    }
}
impl CustomSkinThemeEditorSchema {
    pub fn fields(&self) -> &[CustomSkinThemeEditorFieldSchema] {
        &self.fields
    }
}
impl CustomSkinThemeEditorSchema {
    pub fn exposes_field(&self, stable_key: &str) -> bool {
        self.fields
            .iter()
            .any(|field| field.stable_key == stable_key)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSkinThemeEditorFieldSchema {
    stable_key: &'static str,
    required: bool,
}

impl CustomSkinThemeEditorFieldSchema {
    pub const fn required(stable_key: &'static str) -> Self {
        Self {
            stable_key,
            required: true,
        }
    }
}
impl CustomSkinThemeEditorFieldSchema {
    pub const fn stable_key(&self) -> &'static str {
        self.stable_key
    }
}
impl CustomSkinThemeEditorFieldSchema {
    pub const fn required_flag(&self) -> bool {
        self.required
    }
}

#[cfg(test)]
#[path = "custom_skin_theme_editor_schema_tests.rs"]
mod tests;
