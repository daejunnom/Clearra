use clearra_app::AppResponse;

use super::first_field;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiFumenPanel {
    label_i18n_key: &'static str,
    fumen_like_available: bool,
    fumen_like: Option<String>,
    unsupported_reason: String,
}

impl GuiFumenPanel {
    pub fn from_response(response: &AppResponse) -> Self {
        let fumen_like = first_field(response, &["fumen_like", "fumen", "encoded_fumen"]);
        Self {
            label_i18n_key: "ui.result.fumen",
            fumen_like_available: fumen_like.is_some(),
            fumen_like,
            unsupported_reason: first_field(response, &["fumen_unsupported_reason"])
                .unwrap_or_else(|| "not_connected".to_owned()),
        }
    }
}
impl GuiFumenPanel {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiFumenPanel {
    pub const fn fumen_like_available(&self) -> bool {
        self.fumen_like_available
    }
}
impl GuiFumenPanel {
    pub fn fumen_like(&self) -> Option<&str> {
        self.fumen_like.as_deref()
    }
}
impl GuiFumenPanel {
    pub fn unsupported_reason(&self) -> &str {
        &self.unsupported_reason
    }
}
