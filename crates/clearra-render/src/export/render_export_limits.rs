use crate::RenderError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderExportLimits {
    max_frame_width: u32,
    max_frame_height: u32,
    max_frame_pixels: u64,
    max_gif_frames: usize,
    max_timeline_pixels: u64,
    max_frame_delay_ms: u16,
}

impl RenderExportLimits {
    pub const fn product_default() -> Self {
        Self {
            max_frame_width: 1920,
            max_frame_height: 1080,
            max_frame_pixels: 1920 * 1080,
            max_gif_frames: 240,
            max_timeline_pixels: 1920 * 1080 * 240,
            max_frame_delay_ms: 5000,
        }
    }
}
impl RenderExportLimits {
    pub const fn tight_for_tests() -> Self {
        Self {
            max_frame_width: 64,
            max_frame_height: 64,
            max_frame_pixels: 64 * 64,
            max_gif_frames: 8,
            max_timeline_pixels: 64 * 64 * 8,
            max_frame_delay_ms: 1000,
        }
    }
}
impl RenderExportLimits {
    pub const fn max_frame_width(self) -> u32 {
        self.max_frame_width
    }
}
impl RenderExportLimits {
    pub const fn max_frame_height(self) -> u32 {
        self.max_frame_height
    }
}
impl RenderExportLimits {
    pub const fn max_frame_pixels(self) -> u64 {
        self.max_frame_pixels
    }
}
impl RenderExportLimits {
    pub const fn max_gif_frames(self) -> usize {
        self.max_gif_frames
    }
}
impl RenderExportLimits {
    pub const fn max_timeline_pixels(self) -> u64 {
        self.max_timeline_pixels
    }
}
impl RenderExportLimits {
    pub const fn max_frame_delay_ms(self) -> u16 {
        self.max_frame_delay_ms
    }
}
impl RenderExportLimits {
    pub fn validate_frame(self, width: u32, height: u32) -> Result<(), RenderError> {
        if width > self.max_frame_width {
            return Err(RenderError::ExportLimitExceeded {
                limit: "max_frame_width",
                actual: u64::from(width),
                max: u64::from(self.max_frame_width),
            });
        }
        if height > self.max_frame_height {
            return Err(RenderError::ExportLimitExceeded {
                limit: "max_frame_height",
                actual: u64::from(height),
                max: u64::from(self.max_frame_height),
            });
        }

        let pixels = u64::from(width) * u64::from(height);
        if pixels > self.max_frame_pixels {
            return Err(RenderError::ExportLimitExceeded {
                limit: "max_frame_pixels",
                actual: pixels,
                max: self.max_frame_pixels,
            });
        }

        Ok(())
    }
}
impl RenderExportLimits {
    pub fn validate_timeline(
        self,
        frame_count: usize,
        width: u32,
        height: u32,
        delay_ms: u16,
    ) -> Result<(), RenderError> {
        if frame_count == 0 {
            return Err(RenderError::EmptyTimeline);
        }
        if frame_count > self.max_gif_frames {
            return Err(RenderError::ExportLimitExceeded {
                limit: "max_gif_frames",
                actual: frame_count as u64,
                max: self.max_gif_frames as u64,
            });
        }
        if delay_ms > self.max_frame_delay_ms {
            return Err(RenderError::ExportLimitExceeded {
                limit: "max_frame_delay_ms",
                actual: u64::from(delay_ms),
                max: u64::from(self.max_frame_delay_ms),
            });
        }

        self.validate_frame(width, height)?;
        let timeline_pixels = u64::from(width) * u64::from(height) * frame_count as u64;
        if timeline_pixels > self.max_timeline_pixels {
            return Err(RenderError::ExportLimitExceeded {
                limit: "max_timeline_pixels",
                actual: timeline_pixels,
                max: self.max_timeline_pixels,
            });
        }

        Ok(())
    }
}
