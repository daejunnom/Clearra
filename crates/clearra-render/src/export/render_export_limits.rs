use core::mem::size_of;

use crate::RenderError;

const RGBA_BYTES_PER_PIXEL: u128 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderExportLimits {
    max_frame_width: u32,
    max_frame_height: u32,
    max_frame_pixels: u64,
    max_gif_frames: usize,
    max_timeline_pixels: u64,
    max_frame_delay_ms: u16,
    max_materialization_bytes: u64,
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
            max_materialization_bytes: 256 * 1024 * 1024,
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
            max_materialization_bytes: (64 * 64 * 4 + size_of::<Vec<u8>>() as u64) * 8,
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
    pub const fn max_materialization_bytes(self) -> u64 {
        self.max_materialization_bytes
    }
}
impl RenderExportLimits {
    pub fn validate_frame(self, width: u32, height: u32) -> Result<(), RenderError> {
        let pixels = self.validate_frame_shape(u128::from(width), u128::from(height))?;
        self.validate_materialization_bytes(pixels.saturating_mul(RGBA_BYTES_PER_PIXEL))
    }

    pub(crate) fn validated_scaled_frame(
        self,
        cell_width: usize,
        cell_height: usize,
        cell_size: u32,
    ) -> Result<(u32, u32, usize), RenderError> {
        if cell_size == 0 {
            return Err(RenderError::ExportLimitExceeded {
                limit: "cell_size",
                actual: 0,
                max: u64::from(u16::MAX),
            });
        }

        let width = (cell_width as u128).saturating_mul(u128::from(cell_size));
        let height = (cell_height as u128).saturating_mul(u128::from(cell_size));
        let pixels = self.validate_frame_shape(width, height)?;
        self.validate_materialization_bytes(pixels.saturating_mul(RGBA_BYTES_PER_PIXEL))?;

        let rgba_bytes = pixels.saturating_mul(RGBA_BYTES_PER_PIXEL);
        Ok((
            u32::try_from(width).map_err(|_| self.width_limit_error(width))?,
            u32::try_from(height).map_err(|_| self.height_limit_error(height))?,
            usize::try_from(rgba_bytes)
                .map_err(|_| self.materialization_limit_error(rgba_bytes))?,
        ))
    }

    pub(crate) fn board_cell_capacity<T>(
        self,
        width: usize,
        height: usize,
    ) -> Result<usize, RenderError> {
        let pixels = self.validate_frame_shape(width as u128, height as u128)?;
        let bytes = pixels.saturating_mul(size_of::<T>() as u128);
        self.validate_materialization_bytes(bytes)?;
        usize::try_from(pixels).map_err(|_| self.materialization_limit_error(bytes))
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
                actual: usize_as_u64(frame_count),
                max: usize_as_u64(self.max_gif_frames),
            });
        }
        if delay_ms > self.max_frame_delay_ms {
            return Err(RenderError::ExportLimitExceeded {
                limit: "max_frame_delay_ms",
                actual: u64::from(delay_ms),
                max: u64::from(self.max_frame_delay_ms),
            });
        }

        let frame_pixels = self.validate_frame_shape(u128::from(width), u128::from(height))?;
        let timeline_pixels = frame_pixels.saturating_mul(frame_count as u128);
        if timeline_pixels > u128::from(self.max_timeline_pixels) {
            return Err(RenderError::ExportLimitExceeded {
                limit: "max_timeline_pixels",
                actual: report_u128(timeline_pixels),
                max: self.max_timeline_pixels,
            });
        }

        let rgba_bytes = timeline_pixels.saturating_mul(RGBA_BYTES_PER_PIXEL);
        let frame_carrier_bytes =
            (frame_count as u128).saturating_mul(size_of::<Vec<u8>>() as u128);
        self.validate_materialization_bytes(rgba_bytes.saturating_add(frame_carrier_bytes))
    }

    fn validate_frame_shape(self, width: u128, height: u128) -> Result<u128, RenderError> {
        if width > u128::from(self.max_frame_width) {
            return Err(self.width_limit_error(width));
        }
        if height > u128::from(self.max_frame_height) {
            return Err(self.height_limit_error(height));
        }

        let pixels = width.saturating_mul(height);
        if pixels > u128::from(self.max_frame_pixels) {
            return Err(RenderError::ExportLimitExceeded {
                limit: "max_frame_pixels",
                actual: report_u128(pixels),
                max: self.max_frame_pixels,
            });
        }
        Ok(pixels)
    }

    fn validate_materialization_bytes(self, bytes: u128) -> Result<(), RenderError> {
        if bytes > u128::from(self.max_materialization_bytes) {
            return Err(self.materialization_limit_error(bytes));
        }
        Ok(())
    }

    fn width_limit_error(self, actual: u128) -> RenderError {
        RenderError::ExportLimitExceeded {
            limit: "max_frame_width",
            actual: report_u128(actual),
            max: u64::from(self.max_frame_width),
        }
    }

    fn height_limit_error(self, actual: u128) -> RenderError {
        RenderError::ExportLimitExceeded {
            limit: "max_frame_height",
            actual: report_u128(actual),
            max: u64::from(self.max_frame_height),
        }
    }

    fn materialization_limit_error(self, actual: u128) -> RenderError {
        RenderError::ExportLimitExceeded {
            limit: "max_materialization_bytes",
            actual: report_u128(actual),
            max: self.max_materialization_bytes,
        }
    }
}

#[cfg(test)]
impl RenderExportLimits {
    pub(crate) const fn with_max_frame_pixels_for_test(mut self, max: u64) -> Self {
        self.max_frame_pixels = max;
        self
    }

    pub(crate) const fn with_max_materialization_bytes_for_test(mut self, max: u64) -> Self {
        self.max_materialization_bytes = max;
        self
    }
}

fn report_u128(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn usize_as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
