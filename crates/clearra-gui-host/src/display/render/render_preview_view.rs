use clearra_app::AppResponse;

use super::{RenderCapabilityView, SkinSelectorView};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderPreviewView {
    label_i18n_key: &'static str,
    preview_available: bool,
    preview_status: String,
    capability: RenderCapabilityView,
    skin_selector: SkinSelectorView,
}

impl RenderPreviewView {
    pub fn from_response(response: &AppResponse) -> Self {
        let capability = RenderCapabilityView::from_response(response);
        let skin_selector = SkinSelectorView::from_response(response);
        let preview_available = capability.supported() && capability.render_exact();
        let preview_status = if preview_available {
            "available".to_owned()
        } else {
            capability.unsupported_reason().to_owned()
        };

        Self {
            label_i18n_key: "ui.result.render.preview",
            preview_available,
            preview_status,
            capability,
            skin_selector,
        }
    }
}
impl RenderPreviewView {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl RenderPreviewView {
    pub const fn preview_available(&self) -> bool {
        self.preview_available
    }
}
impl RenderPreviewView {
    pub fn preview_status(&self) -> &str {
        &self.preview_status
    }
}
impl RenderPreviewView {
    pub const fn capability(&self) -> &RenderCapabilityView {
        &self.capability
    }
}
impl RenderPreviewView {
    pub const fn skin_selector(&self) -> &SkinSelectorView {
        &self.skin_selector
    }
}
