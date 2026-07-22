use crate::RenderError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PngEncoder;

impl PngEncoder {
    pub fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, RenderError> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .map(|height| width * height * 4)
            })
            .ok_or_else(|| RenderError::PngEncodingFailed {
                reason: "rgba_dimensions_overflow".to_owned(),
            })?;
        if rgba.len() != expected {
            return Err(RenderError::PngEncodingFailed {
                reason: "rgba_buffer_length_mismatch".to_owned(),
            });
        }

        let mut output = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut output, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer =
                encoder
                    .write_header()
                    .map_err(|error| RenderError::PngEncodingFailed {
                        reason: error.to_string(),
                    })?;
            writer
                .write_image_data(rgba)
                .map_err(|error| RenderError::PngEncodingFailed {
                    reason: error.to_string(),
                })?;
            writer
                .finish()
                .map_err(|error| RenderError::PngEncodingFailed {
                    reason: error.to_string(),
                })?;
        }
        Ok(output)
    }
}
