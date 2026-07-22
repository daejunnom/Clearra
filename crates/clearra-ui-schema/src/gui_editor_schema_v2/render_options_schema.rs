#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderOptionsSchema {
    option_fields: Vec<&'static str>,
    capability_fields: Vec<&'static str>,
    unsupported_reason_required: bool,
}

impl RenderOptionsSchema {
    pub fn v2() -> Self {
        Self {
            option_fields: vec![
                "renderer",
                "frame_format",
                "skin_id",
                "timeline",
                "gif_animation",
                "export_limits",
            ],
            capability_fields: vec![
                "renderer_capability",
                "supported",
                "render_exact",
                "unsupported_reason",
                "skin_manifest_valid",
                "atlas_provenance_valid",
                "max_frame_width",
                "max_frame_height",
                "max_gif_frames",
                "max_frame_delay_ms",
            ],
            unsupported_reason_required: false,
        }
    }
}
impl RenderOptionsSchema {
    pub fn option_fields(&self) -> &[&'static str] {
        &self.option_fields
    }
}
impl RenderOptionsSchema {
    pub fn capability_fields(&self) -> &[&'static str] {
        &self.capability_fields
    }
}
impl RenderOptionsSchema {
    pub const fn unsupported_reason_required(&self) -> bool {
        self.unsupported_reason_required
    }
}
impl RenderOptionsSchema {
    pub fn exposes_capability_field(&self, field: &str) -> bool {
        self.capability_fields.iter().any(|known| known == &field)
    }
}
