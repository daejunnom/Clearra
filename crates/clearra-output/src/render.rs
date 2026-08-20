use clearra_fumen::codec::{FumenLikeTrace, FumenLikeWriter};
use clearra_render::{ExactBitmapRenderer, RenderCapabilityReport, RenderExportLimits};

use crate::{
    json::json_writer::JsonWriter,
    model::render_message::RenderMessage,
    text::{text_output_profile::TextOutputProfile, text_writer::TextWriter},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenderFormat {
    #[default]
    Text,
    TextVerbose,
    TextDiagnostics,
    Json,
    FumenLike,
}

impl RenderFormat {
    pub fn with_text_profile(self, profile: TextOutputProfile) -> Self {
        match self {
            Self::Text | Self::TextVerbose | Self::TextDiagnostics => match profile {
                TextOutputProfile::HumanSummary => Self::Text,
                TextOutputProfile::Verbose => Self::TextVerbose,
                TextOutputProfile::Diagnostics => Self::TextDiagnostics,
            },
            Self::Json | Self::FumenLike => self,
        }
    }
}
impl RenderFormat {
    fn text_profile(self) -> Option<TextOutputProfile> {
        match self {
            Self::Text => Some(TextOutputProfile::HumanSummary),
            Self::TextVerbose => Some(TextOutputProfile::Verbose),
            Self::TextDiagnostics => Some(TextOutputProfile::Diagnostics),
            Self::Json | Self::FumenLike => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactBitmapOutputFormat {
    Png,
    Gif,
}

impl ExactBitmapOutputFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Gif => "gif",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactBitmapOutput {
    format: ExactBitmapOutputFormat,
    bytes: Vec<u8>,
    render_exact: bool,
    skin_id: &'static str,
}

impl ExactBitmapOutput {
    pub const fn format(&self) -> ExactBitmapOutputFormat {
        self.format
    }
}
impl ExactBitmapOutput {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl ExactBitmapOutput {
    pub const fn render_exact(&self) -> bool {
        self.render_exact
    }
}
impl ExactBitmapOutput {
    pub const fn skin_id(&self) -> &'static str {
        self.skin_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitmapExportLimitReport {
    max_frame_width: u32,
    max_frame_height: u32,
    max_gif_frames: u32,
    max_frame_delay_ms: u16,
    renderer: &'static str,
}

impl BitmapExportLimitReport {
    pub const fn product_default() -> Self {
        Self {
            max_frame_width: 1920,
            max_frame_height: 1080,
            max_gif_frames: 240,
            max_frame_delay_ms: 5000,
            renderer: "clearra-render-exact-bitmap",
        }
    }
}
impl BitmapExportLimitReport {
    pub const fn max_frame_width(self) -> u32 {
        self.max_frame_width
    }
}
impl BitmapExportLimitReport {
    pub const fn max_frame_height(self) -> u32 {
        self.max_frame_height
    }
}
impl BitmapExportLimitReport {
    pub const fn max_gif_frames(self) -> u32 {
        self.max_gif_frames
    }
}
impl BitmapExportLimitReport {
    pub const fn max_frame_delay_ms(self) -> u16 {
        self.max_frame_delay_ms
    }
}
impl BitmapExportLimitReport {
    pub const fn renderer(self) -> &'static str {
        self.renderer
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderExactOutputGate;

impl RenderExactOutputGate {
    pub fn render_replay_trace(
        trace: &clearra_replay::ReplayTrace,
        format: ExactBitmapOutputFormat,
    ) -> Result<ExactBitmapOutput, clearra_render::RenderError> {
        let limits = RenderExportLimits::product_default();
        let bytes = match format {
            ExactBitmapOutputFormat::Png => {
                ExactBitmapRenderer::render_replay_png(trace, 16, limits)?
            }
            ExactBitmapOutputFormat::Gif => {
                ExactBitmapRenderer::render_replay_timeline_gif(trace, 16, 160, limits)?
            }
        };
        Ok(ExactBitmapOutput {
            format,
            bytes,
            render_exact: true,
            skin_id: "default",
        })
    }

    pub fn capability_report() -> RenderCapabilityReport {
        RenderCapabilityReport::current()
    }
}
impl RenderExactOutputGate {
    pub const fn bitmap_export_limits() -> BitmapExportLimitReport {
        BitmapExportLimitReport::product_default()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderFormatDispatcher;

impl RenderFormatDispatcher {
    pub fn render(
        message: &RenderMessage,
        format: RenderFormat,
    ) -> Result<String, clearra_fumen::codec::FumenLikeWriteError> {
        match format {
            RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
                Ok(TextWriter::lines(&message.text_lines_with_profile(
                    format.text_profile().expect("text profile"),
                )))
            }
            RenderFormat::Json => Ok(JsonWriter::write(&message.json_contract())),
            RenderFormat::FumenLike => {
                FumenLikeWriter::write(&FumenLikeTrace::new(message.fumen_pages()))
            }
        }
    }
}
impl RenderFormatDispatcher {
    pub fn render_replay_trace(
        trace: &clearra_replay::ReplayTrace,
        format: RenderFormat,
    ) -> Result<String, clearra_fumen::codec::FumenLikeWriteError> {
        match format {
            RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
                Ok(TextWriter::replay_trace(trace))
            }
            RenderFormat::Json => Ok(JsonWriter::write(
                &crate::json::JsonContract::from_replay_trace(trace),
            )),
            RenderFormat::FumenLike => FumenLikeWriter::write_replay_trace(trace),
        }
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
