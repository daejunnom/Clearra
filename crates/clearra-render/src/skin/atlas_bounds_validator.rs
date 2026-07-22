use serde_json::Value;

pub struct AtlasBoundsValidator;

impl AtlasBoundsValidator {
    pub fn validate_piece_rects(
        manifest: &Value,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<(), String> {
        Self::validate_rect_group(manifest, "pieces", atlas_width, atlas_height)
    }

    pub fn validate_rect_group(
        manifest: &Value,
        group: &str,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<(), String> {
        let pieces = manifest
            .get(group)
            .and_then(Value::as_object)
            .ok_or_else(|| format!("missing_{group}_object"))?;

        for (piece_id, rect) in pieces {
            Self::validate_piece_rect(piece_id, rect, atlas_width, atlas_height)?;
        }

        Ok(())
    }
}
impl AtlasBoundsValidator {
    fn validate_piece_rect(
        piece_id: &str,
        rect: &Value,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<(), String> {
        let x = required_u32(rect, piece_id, "x")?;
        let y = required_u32(rect, piece_id, "y")?;
        let width = required_positive_u32(rect, piece_id, "width")?;
        let height = required_positive_u32(rect, piece_id, "height")?;

        let right = x
            .checked_add(width)
            .ok_or_else(|| format!("piece_rect_out_of_bounds:{piece_id}"))?;
        let bottom = y
            .checked_add(height)
            .ok_or_else(|| format!("piece_rect_out_of_bounds:{piece_id}"))?;

        if right > atlas_width || bottom > atlas_height {
            return Err(format!("piece_rect_out_of_bounds:{piece_id}"));
        }

        Ok(())
    }
}

fn required_u32(rect: &Value, piece_id: &str, field: &str) -> Result<u32, String> {
    rect.get(field)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("missing_rect_field:{piece_id}:{field}"))
}

fn required_positive_u32(rect: &Value, piece_id: &str, field: &str) -> Result<u32, String> {
    let value = required_u32(rect, piece_id, field)?;
    if value == 0 {
        return Err(format!("invalid_rect_field:{piece_id}:{field}"));
    }
    Ok(value)
}
