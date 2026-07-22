use clearra_app::AppOutputPolicy;
use clearra_output::RenderFormat;

use crate::model::{GuiOutputForm, GuiOutputFormat};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutputRequestBuilder;

impl OutputRequestBuilder {
    pub fn build(form: &GuiOutputForm) -> GuiOutputRequestBuild {
        let render_format = match form.format() {
            GuiOutputFormat::Text => {
                if form.diagnostics() {
                    RenderFormat::TextDiagnostics
                } else if form.verbose() {
                    RenderFormat::TextVerbose
                } else {
                    RenderFormat::Text
                }
            }
            GuiOutputFormat::Json => RenderFormat::Json,
            GuiOutputFormat::FumenLike => RenderFormat::FumenLike,
            GuiOutputFormat::Render => RenderFormat::Text,
        };
        GuiOutputRequestBuild {
            output_policy: AppOutputPolicy::new(true),
            render_format,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiOutputRequestBuild {
    output_policy: AppOutputPolicy,
    render_format: RenderFormat,
}

impl GuiOutputRequestBuild {
    pub fn output_policy(&self) -> &AppOutputPolicy {
        &self.output_policy
    }
}
impl GuiOutputRequestBuild {
    pub const fn render_format(&self) -> RenderFormat {
        self.render_format
    }
}
impl GuiOutputRequestBuild {
    pub fn into_output_policy(self) -> AppOutputPolicy {
        self.output_policy
    }
}
