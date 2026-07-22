use clearra_app::AppResponse;

use super::{FumenCopyButtonModel, FumenPageListView};
use crate::display::first_field;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FumenOutputView {
    label_i18n_key: &'static str,
    fumen_like_exists: bool,
    encoded_output: Option<String>,
    unsupported_reason: String,
    copy_button: FumenCopyButtonModel,
    page_list: FumenPageListView,
}

impl FumenOutputView {
    pub fn from_response(response: &AppResponse) -> Self {
        let encoded_output = first_field(response, &["fumen_like", "fumen", "encoded_fumen"]);
        let unsupported_reason = first_field(
            response,
            &["fumen_unsupported_reason", "fumen_like_unsupported_reason"],
        )
        .unwrap_or_else(|| {
            if encoded_output.is_some() {
                "none".to_owned()
            } else {
                "not_connected".to_owned()
            }
        });
        let copy_button = FumenCopyButtonModel::from_payload(encoded_output.as_deref());
        let page_list = FumenPageListView::from_payload(encoded_output.as_deref());

        Self {
            label_i18n_key: "ui.result.fumen.output",
            fumen_like_exists: encoded_output.is_some(),
            encoded_output,
            unsupported_reason,
            copy_button,
            page_list,
        }
    }
}
impl FumenOutputView {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl FumenOutputView {
    pub const fn fumen_like_exists(&self) -> bool {
        self.fumen_like_exists
    }
}
impl FumenOutputView {
    pub fn encoded_output(&self) -> Option<&str> {
        self.encoded_output.as_deref()
    }
}
impl FumenOutputView {
    pub fn unsupported_reason(&self) -> &str {
        &self.unsupported_reason
    }
}
impl FumenOutputView {
    pub const fn copy_button(&self) -> &FumenCopyButtonModel {
        &self.copy_button
    }
}
impl FumenOutputView {
    pub const fn page_list(&self) -> &FumenPageListView {
        &self.page_list
    }
}
