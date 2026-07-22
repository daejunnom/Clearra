#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderFrameFormat {
    Png,
    Gif,
}

impl RenderFrameFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Gif => "gif",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderUnsupportedReason {
    MissingValidatedSkin,
    InvalidSkinAsset,
}

impl RenderUnsupportedReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingValidatedSkin => "missing_validated_skin",
            Self::InvalidSkinAsset => "invalid_skin_asset",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderCapability {
    frame_format: RenderFrameFormat,
    supported: bool,
    render_exact: bool,
    unsupported_reason: Option<RenderUnsupportedReason>,
}

impl RenderCapability {
    pub const fn connected_exact(frame_format: RenderFrameFormat) -> Self {
        Self {
            frame_format,
            supported: true,
            render_exact: true,
            unsupported_reason: None,
        }
    }

    pub const fn unsupported(
        frame_format: RenderFrameFormat,
        reason: RenderUnsupportedReason,
    ) -> Self {
        Self {
            frame_format,
            supported: false,
            render_exact: false,
            unsupported_reason: Some(reason),
        }
    }

    pub const fn frame_format(self) -> RenderFrameFormat {
        self.frame_format
    }

    pub const fn supported(self) -> bool {
        self.supported
    }

    pub const fn render_exact(self) -> bool {
        self.render_exact
    }

    pub const fn unsupported_reason(self) -> Option<RenderUnsupportedReason> {
        self.unsupported_reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderCapabilityReport {
    frame_formats: Vec<RenderCapability>,
}

impl RenderCapabilityReport {
    pub fn current() -> Self {
        let capability = |format| {
            if crate::SkinAtlas::builtin_default().is_ok() {
                RenderCapability::connected_exact(format)
            } else {
                RenderCapability::unsupported(format, RenderUnsupportedReason::MissingValidatedSkin)
            }
        };
        Self {
            frame_formats: vec![
                capability(RenderFrameFormat::Png),
                capability(RenderFrameFormat::Gif),
            ],
        }
    }

    pub fn frame_formats(&self) -> &[RenderCapability] {
        &self.frame_formats
    }

    pub fn capability_for(&self, frame_format: RenderFrameFormat) -> Option<RenderCapability> {
        self.frame_formats
            .iter()
            .copied()
            .find(|capability| capability.frame_format() == frame_format)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderExactnessGate;

impl RenderExactnessGate {
    pub fn request_exact_frame(
        frame_format: RenderFrameFormat,
    ) -> Result<RenderCapability, crate::RenderError> {
        RenderCapabilityReport::current()
            .capability_for(frame_format)
            .filter(|capability| capability.supported() && capability.render_exact())
            .ok_or(crate::RenderError::UnsupportedFrameFormat {
                frame_format,
                reason: RenderUnsupportedReason::MissingValidatedSkin,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_renderer_is_connected_exact() {
        let capability = RenderCapabilityReport::current()
            .capability_for(RenderFrameFormat::Png)
            .expect("png capability");
        assert!(capability.supported());
        assert!(capability.render_exact());
        assert_eq!(capability.unsupported_reason(), None);
    }
}
