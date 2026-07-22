use serde_json::Value;

use super::AtlasBoundsValidator;

pub const STANDARD_PIECES: [&str; 7] = ["I", "O", "T", "S", "Z", "J", "L"];
const REQUIRED_SPECIAL_TILES: [&str; 2] = ["empty", "initial_gray"];

pub struct SkinManifestValidator;

impl SkinManifestValidator {
    pub fn validate_manifest(manifest: &Value) -> Result<(), String> {
        require_u64(manifest, "schema_version", 1)?;
        require_non_empty_string(manifest, "skin_id")?;
        require_non_empty_string(manifest, "atlas_path")?;
        require_string(manifest, "atlas_format", "png")?;
        let atlas_width = require_positive_u32(manifest, "atlas_width")?;
        let atlas_height = require_positive_u32(manifest, "atlas_height")?;
        let tile_width = require_minimum_u32(manifest, "tile_width", 4)?;
        let tile_height = require_minimum_u32(manifest, "tile_height", 4)?;
        require_bool(manifest, "runtime_raw_svg_allowed", false)?;
        Self::validate_standard_piece_mapping(manifest)?;
        validate_special_mapping(manifest)?;
        AtlasBoundsValidator::validate_piece_rects(manifest, atlas_width, atlas_height)?;
        AtlasBoundsValidator::validate_rect_group(manifest, "special", atlas_width, atlas_height)?;
        validate_tile_dimensions(manifest, "pieces", tile_width, tile_height)?;
        validate_tile_dimensions(manifest, "special", tile_width, tile_height)?;
        validate_renderer_capability(manifest)?;
        Ok(())
    }

    pub fn validate_standard_piece_mapping(manifest: &Value) -> Result<(), String> {
        let required_pieces = manifest
            .get("required_pieces")
            .and_then(Value::as_array)
            .ok_or_else(|| "missing_required_pieces".to_owned())?;
        let pieces = manifest
            .get("pieces")
            .and_then(Value::as_object)
            .ok_or_else(|| "missing_pieces_object".to_owned())?;
        for piece in STANDARD_PIECES {
            if !required_pieces
                .iter()
                .any(|value| value.as_str() == Some(piece))
                || !pieces.contains_key(piece)
            {
                return Err(format!("missing_piece:{piece}"));
            }
        }
        Ok(())
    }
}

fn validate_special_mapping(manifest: &Value) -> Result<(), String> {
    let special = manifest
        .get("special")
        .and_then(Value::as_object)
        .ok_or_else(|| "missing_special_tiles".to_owned())?;
    for tile in REQUIRED_SPECIAL_TILES {
        if !special.contains_key(tile) {
            return Err(format!("missing_special_tile:{tile}"));
        }
    }
    Ok(())
}

fn validate_tile_dimensions(
    manifest: &Value,
    group: &str,
    tile_width: u32,
    tile_height: u32,
) -> Result<(), String> {
    let entries = manifest
        .get(group)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("missing_{group}"))?;
    for (key, rect) in entries {
        if rect.get("width").and_then(Value::as_u64) != Some(u64::from(tile_width))
            || rect.get("height").and_then(Value::as_u64) != Some(u64::from(tile_height))
        {
            return Err(format!("tile_dimensions_mismatch:{key}"));
        }
    }
    Ok(())
}

fn validate_renderer_capability(manifest: &Value) -> Result<(), String> {
    let capability = manifest
        .get("capability")
        .ok_or_else(|| "missing_capability".to_owned())?;
    require_bool(capability, "render_exact", true)?;
    require_bool(capability, "supported", true)?;
    if capability.get("unsupported_reason").is_some() {
        return Err("connected_renderer_cannot_have_unsupported_reason".to_owned());
    }
    Ok(())
}

fn require_u64(value: &Value, field: &str, expected: u64) -> Result<(), String> {
    match value.get(field).and_then(Value::as_u64) {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(format!("invalid_{field}")),
        None => Err(format!("missing_{field}")),
    }
}

fn require_positive_u32(value: &Value, field: &str) -> Result<u32, String> {
    require_minimum_u32(value, field, 1)
}

fn require_minimum_u32(value: &Value, field: &str, minimum: u32) -> Result<u32, String> {
    let raw = value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("missing_{field}"))?;
    if raw < minimum {
        return Err(format!("invalid_{field}"));
    }
    Ok(raw)
}

fn require_bool(value: &Value, field: &str, expected: bool) -> Result<(), String> {
    match value.get(field).and_then(Value::as_bool) {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(format!("invalid_{field}")),
        None => Err(format!("missing_{field}")),
    }
}

fn require_string(value: &Value, field: &str, expected: &str) -> Result<(), String> {
    match value.get(field).and_then(Value::as_str) {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(format!("invalid_{field}")),
        None => Err(format!("missing_{field}")),
    }
}

fn require_non_empty_string(value: &Value, field: &str) -> Result<(), String> {
    match value.get(field).and_then(Value::as_str) {
        Some(actual) if !actual.trim().is_empty() => Ok(()),
        Some(_) => Err(format!("invalid_{field}")),
        None => Err(format!("missing_{field}")),
    }
}
