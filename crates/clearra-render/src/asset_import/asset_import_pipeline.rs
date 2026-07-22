use std::time::Instant;

use resvg::{tiny_skia, usvg};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::bitmap::png_encoder::PngEncoder;

use super::{sanitize_svg, AssetImportLimits, AssetImportReportValidator};

const STANDARD_TILE_KEYS: [&str; 9] = ["empty", "initial_gray", "I", "O", "T", "S", "Z", "J", "L"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetImportMetadata {
    pub source_label: String,
    pub origin_kind: String,
    pub skin_id: String,
    pub display_name: String,
    pub license: String,
    pub redistribution: String,
    pub tile_width: u32,
    pub tile_height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetImportBundle {
    sanitized_svg: Vec<u8>,
    atlas_png: Vec<u8>,
    manifest_json: Vec<u8>,
    provenance_json: Vec<u8>,
    import_report_json: Vec<u8>,
}

impl AssetImportBundle {
    pub fn sanitized_svg(&self) -> &[u8] {
        &self.sanitized_svg
    }

    pub fn atlas_png(&self) -> &[u8] {
        &self.atlas_png
    }

    pub fn manifest_json(&self) -> &[u8] {
        &self.manifest_json
    }

    pub fn provenance_json(&self) -> &[u8] {
        &self.provenance_json
    }

    pub fn import_report_json(&self) -> &[u8] {
        &self.import_report_json
    }

    pub fn verify_hashes(&self, original_svg: &[u8]) -> Result<(), String> {
        let provenance: Value = serde_json::from_slice(&self.provenance_json)
            .map_err(|_| "invalid_generated_provenance".to_owned())?;
        for (field, bytes) in [
            ("original_file_sha256", original_svg),
            ("sanitized_svg_sha256", self.sanitized_svg.as_slice()),
            ("atlas_png_sha256", self.atlas_png.as_slice()),
            ("manifest_sha256", self.manifest_json.as_slice()),
        ] {
            if provenance.get(field).and_then(Value::as_str) != Some(&sha256(bytes)) {
                return Err(format!("provenance_hash_mismatch:{field}"));
            }
        }
        Ok(())
    }
}

pub struct AssetImportPipeline;

impl AssetImportPipeline {
    pub fn import_svg(
        source: &[u8],
        metadata: &AssetImportMetadata,
        limits: &AssetImportLimits,
    ) -> Result<AssetImportBundle, String> {
        let started = Instant::now();
        if source.starts_with(&[0x1f, 0x8b]) {
            return Err("compressed_svg_input_forbidden".to_owned());
        }
        let source_text = std::str::from_utf8(source).map_err(|_| "svg_must_be_utf8".to_owned())?;
        let sanitized = sanitize_svg(source_text, limits)?;
        enforce_time(started, limits)?;
        let (width, height, atlas_png) = rasterize(&sanitized, limits)?;
        let expected_width = metadata
            .tile_width
            .checked_mul(STANDARD_TILE_KEYS.len() as u32)
            .ok_or_else(|| "atlas_dimensions_overflow".to_owned())?;
        if width != expected_width || height != metadata.tile_height {
            return Err(format!(
                "standard_atlas_dimensions_required:{width}x{height}:{expected_width}x{}",
                metadata.tile_height
            ));
        }
        enforce_time(started, limits)?;

        let manifest = manifest_value(metadata, width, height);
        let manifest_json = pretty_json(&manifest)?;
        let original_hash = sha256(source);
        let sanitized_hash = sha256(sanitized.as_bytes());
        let atlas_hash = sha256(&atlas_png);
        let manifest_hash = sha256(&manifest_json);
        let provenance = json!({
            "schema_version": 1,
            "skin_id": metadata.skin_id,
            "asset_kind": "reviewed-svg-rasterized-atlas",
            "source": metadata.source_label,
            "raw_svg_runtime_rendering": false,
            "svg_import_policy": "sanitize-rasterize-at-build-time",
            "atlas_path": "atlas.png",
            "atlas_format": "png",
            "original_file_sha256": original_hash,
            "sanitized_svg_sha256": sanitized_hash,
            "atlas_png_sha256": atlas_hash,
            "manifest_sha256": manifest_hash,
            "license": metadata.license,
            "redistribution": metadata.redistribution,
            "import_tool_version": "clearra-asset-import-v1"
        });
        let provenance_json = pretty_json(&provenance)?;
        let report = json!({
            "schema_version": 1,
            "kind": "asset-import-security-report",
            "source_label": metadata.source_label,
            "origin_kind": metadata.origin_kind,
            "original_file_sha256": original_hash,
            "sanitized_svg_sha256": sanitized_hash,
            "atlas_png_sha256": atlas_hash,
            "manifest_sha256": manifest_hash,
            "provenance_sha256": sha256(&provenance_json),
            "license": metadata.license,
            "redistribution": metadata.redistribution,
            "import_tool_version": "clearra-asset-import-v1",
            "security_report_id": format!("clearra-asset-{}", &manifest_hash[..16]),
            "limits": limit_value(limits)
        });
        AssetImportReportValidator::validate_import_report(&report)?;
        let bundle = AssetImportBundle {
            sanitized_svg: sanitized.into_bytes(),
            atlas_png,
            manifest_json,
            provenance_json,
            import_report_json: pretty_json(&report)?,
        };
        bundle.verify_hashes(source)?;
        enforce_time(started, limits)?;
        Ok(bundle)
    }
}

pub fn rasterize_sanitized_svg(svg: &str, limits: &AssetImportLimits) -> Result<Vec<u8>, String> {
    super::SvgSecurityScanner::validate_svg(svg, limits)?;
    rasterize(svg, limits).map(|(_, _, png)| png)
}

fn rasterize(svg: &str, limits: &AssetImportLimits) -> Result<(u32, u32, Vec<u8>), String> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &options).map_err(|error| error.to_string())?;
    let width = tree.size().width().round() as u32;
    let height = tree.size().height().round() as u32;
    if width == 0 || height == 0 {
        return Err("svg_raster_dimensions_empty".to_owned());
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limits.max_raster_pixels {
        return Err("svg_raster_pixel_limit_exceeded".to_owned());
    }
    let memory = pixels.saturating_mul(4);
    if memory > limits.max_memory_mib.saturating_mul(1024 * 1024) {
        return Err("svg_memory_limit_exceeded".to_owned());
    }
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "svg_pixmap_allocation_failed".to_owned())?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.data().to_vec();
    unpremultiply_rgba(&mut rgba);
    let png = PngEncoder::encode_rgba(width, height, &rgba)
        .map_err(|error| format!("svg_png_encode_failed:{error:?}"))?;
    Ok((width, height, png))
}

fn unpremultiply_rgba(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
}

fn manifest_value(metadata: &AssetImportMetadata, width: u32, height: u32) -> Value {
    let tile = |index: u32| {
        json!({
            "x": index * metadata.tile_width,
            "y": 0,
            "width": metadata.tile_width,
            "height": metadata.tile_height
        })
    };
    json!({
        "schema_version": 1,
        "skin_id": metadata.skin_id,
        "display_name": metadata.display_name,
        "atlas_path": "atlas.png",
        "atlas_format": "png",
        "atlas_width": width,
        "atlas_height": height,
        "tile_width": metadata.tile_width,
        "tile_height": metadata.tile_height,
        "runtime_raw_svg_allowed": false,
        "required_pieces": ["I", "O", "T", "S", "Z", "J", "L"],
        "pieces": {
            "I": tile(2), "O": tile(3), "T": tile(4), "S": tile(5),
            "Z": tile(6), "J": tile(7), "L": tile(8)
        },
        "special": { "empty": tile(0), "initial_gray": tile(1) },
        "capability": { "render_exact": true, "supported": true }
    })
}

fn limit_value(limits: &AssetImportLimits) -> Value {
    json!({
        "max_svg_bytes": limits.max_svg_bytes,
        "max_decompressed_bytes": limits.max_decompressed_bytes,
        "max_elements": limits.max_elements,
        "max_group_depth": limits.max_group_depth,
        "max_path_commands": limits.max_path_commands,
        "max_path_segments_per_path": limits.max_path_segments_per_path,
        "max_gradients": limits.max_gradients,
        "max_filters": limits.max_filters,
        "max_external_references": limits.max_external_references,
        "max_css_rules": limits.max_css_rules,
        "max_viewbox_width": limits.max_viewbox_width,
        "max_viewbox_height": limits.max_viewbox_height,
        "max_raster_pixels": limits.max_raster_pixels,
        "max_import_time_ms": limits.max_import_time_ms,
        "max_memory_mib": limits.max_memory_mib
    })
}

fn pretty_json(value: &Value) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn enforce_time(started: Instant, limits: &AssetImportLimits) -> Result<(), String> {
    if started.elapsed().as_millis() > u128::from(limits.max_import_time_ms) {
        return Err("svg_import_time_limit_exceeded".to_owned());
    }
    Ok(())
}
