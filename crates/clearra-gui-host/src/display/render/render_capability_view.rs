use clearra_app::AppResponse;

use crate::display::bool_field;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderCapabilityView {
    label_i18n_key: &'static str,
    png_supported: bool,
    gif_supported: bool,
    render_exact: bool,
    unsupported_reason: String,
    capability_source: &'static str,
}

impl RenderCapabilityView {
    pub fn from_response(response: &AppResponse) -> Self {
        let report = response
            .capability_report()
            .render_capability()
            .expect("clearra-app AppResponse must carry the runtime render capability");

        Self {
            label_i18n_key: "ui.result.render.capability",
            png_supported: report.png_supported(),
            gif_supported: report.gif_supported(),
            render_exact: report.render_exact(),
            unsupported_reason: report.unsupported_reason().unwrap_or("none").to_owned(),
            capability_source: "app-response",
        }
    }
}
impl RenderCapabilityView {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl RenderCapabilityView {
    pub const fn supported(&self) -> bool {
        self.png_supported && self.gif_supported
    }

    pub const fn png_supported(&self) -> bool {
        self.png_supported
    }

    pub const fn gif_supported(&self) -> bool {
        self.gif_supported
    }
}
impl RenderCapabilityView {
    pub const fn render_exact(&self) -> bool {
        self.render_exact
    }
}
impl RenderCapabilityView {
    pub fn unsupported_reason(&self) -> &str {
        &self.unsupported_reason
    }
}
impl RenderCapabilityView {
    pub const fn capability_source(&self) -> &'static str {
        self.capability_source
    }
}

pub(crate) fn skin_manifest_valid(response: &AppResponse) -> bool {
    bool_field(response, "skin_manifest_valid") || bool_field(response, "skin_validation_passed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_app::{AppError, AppErrorCode, AppStatus};

    fn product_response() -> AppResponse {
        AppResponse::failed(
            AppStatus::Unsupported,
            AppError::new(AppErrorCode::Unsupported, "test response"),
        )
    }

    #[test]
    fn renderer_capability_matches_runtime_report() {
        let response = product_response();
        let report = response
            .capability_report()
            .render_capability()
            .expect("runtime report");
        let view = RenderCapabilityView::from_response(&response);

        assert_eq!(view.png_supported(), report.png_supported());
        assert_eq!(view.gif_supported(), report.gif_supported());
        assert_eq!(view.render_exact(), report.render_exact());
        assert_eq!(view.unsupported_reason(), "none");
    }

    #[test]
    fn render_ui_matches_runtime_capability() {
        let view = RenderCapabilityView::from_response(&product_response());

        assert!(view.supported());
        assert!(view.render_exact());
        assert_eq!(view.capability_source(), "app-response");
    }
}
