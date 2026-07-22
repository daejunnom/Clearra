use clearra_app::AppResponse;

use super::render_capability_view::skin_manifest_valid;
use crate::display::{field_value, first_field};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkinSelectorView {
    label_i18n_key: &'static str,
    selected_skin_id: String,
    manifest_status: String,
    provenance_status: String,
    atlas_format: String,
}

impl SkinSelectorView {
    pub fn from_response(response: &AppResponse) -> Self {
        let manifest_status = first_field(
            response,
            &["skin_manifest_status", "skin_validation_status"],
        )
        .unwrap_or_else(|| {
            if skin_manifest_valid(response) {
                "valid".to_owned()
            } else {
                "not_checked".to_owned()
            }
        });

        Self {
            label_i18n_key: "ui.result.render.skin",
            selected_skin_id: first_field(response, &["skin_id", "selected_skin_id"])
                .unwrap_or_else(|| "default".to_owned()),
            manifest_status,
            provenance_status: field_value(response, "skin_provenance_status")
                .unwrap_or_else(|| "not_checked".to_owned()),
            atlas_format: field_value(response, "skin_atlas_format")
                .unwrap_or_else(|| "png".to_owned()),
        }
    }
}
impl SkinSelectorView {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl SkinSelectorView {
    pub fn selected_skin_id(&self) -> &str {
        &self.selected_skin_id
    }
}
impl SkinSelectorView {
    pub fn manifest_status(&self) -> &str {
        &self.manifest_status
    }
}
impl SkinSelectorView {
    pub fn provenance_status(&self) -> &str {
        &self.provenance_status
    }
}
impl SkinSelectorView {
    pub fn atlas_format(&self) -> &str {
        &self.atlas_format
    }
}
