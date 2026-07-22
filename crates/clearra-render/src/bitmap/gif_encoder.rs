use gif::{Encoder, Frame, Repeat};

use crate::RenderError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GifEncoder;

impl GifEncoder {
    pub fn encode_rgba_frames(
        width: u16,
        height: u16,
        frames: &[Vec<u8>],
        delay_ms: u16,
    ) -> Result<Vec<u8>, RenderError> {
        let mut output = Vec::new();
        {
            let mut encoder = Encoder::new(&mut output, width, height, &[]).map_err(map_error)?;
            encoder.set_repeat(Repeat::Infinite).map_err(map_error)?;
            for rgba in frames {
                let mut rgba = rgba.clone();
                let mut frame = Frame::from_rgba_speed(width, height, &mut rgba, 10);
                frame.delay = delay_ms.saturating_add(5) / 10;
                encoder.write_frame(&frame).map_err(map_error)?;
            }
        }
        Ok(output)
    }
}

fn map_error(error: gif::EncodingError) -> RenderError {
    RenderError::GifEncodingFailed {
        reason: error.to_string(),
    }
}
