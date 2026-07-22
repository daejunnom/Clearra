use std::{collections::BTreeMap, io::Cursor};

use serde_json::Value;

use crate::{RenderError, RenderTile};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtlasRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl AtlasRect {
    pub const fn x(self) -> u32 {
        self.x
    }

    pub const fn y(self) -> u32 {
        self.y
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkinAtlas {
    skin_id: String,
    path: String,
    width: u32,
    height: u32,
    tile_width: u32,
    tile_height: u32,
    rgba: Vec<u8>,
    tiles: BTreeMap<String, AtlasRect>,
}

impl SkinAtlas {
    pub fn from_manifest_and_png(
        manifest: &Value,
        atlas_bytes: &[u8],
    ) -> Result<Self, RenderError> {
        super::SkinManifestValidator::validate_manifest(manifest)
            .map_err(|reason| RenderError::InvalidSkinManifest { reason })?;
        let (width, height, rgba) = decode_rgba(atlas_bytes)?;
        let expected_width = required_u32(manifest, "atlas_width")?;
        let expected_height = required_u32(manifest, "atlas_height")?;
        if width != expected_width || height != expected_height {
            return Err(RenderError::InvalidSkinAtlas {
                reason: format!(
                    "atlas_dimensions_mismatch:{width}x{height}:{expected_width}x{expected_height}"
                ),
            });
        }

        let mut tiles = BTreeMap::new();
        collect_rects(manifest, "pieces", &mut tiles)?;
        collect_rects(manifest, "special", &mut tiles)?;
        Ok(Self {
            skin_id: required_str(manifest, "skin_id")?.to_owned(),
            path: required_str(manifest, "atlas_path")?.to_owned(),
            width,
            height,
            tile_width: required_u32(manifest, "tile_width")?,
            tile_height: required_u32(manifest, "tile_height")?,
            rgba,
            tiles,
        })
    }

    pub fn builtin_default() -> Result<Self, RenderError> {
        let manifest =
            serde_json::from_str(include_str!("../../../../assets/skins/default/skin.json"))
                .map_err(|error| RenderError::InvalidSkinManifest {
                    reason: error.to_string(),
                })?;
        Self::from_manifest_and_png(
            &manifest,
            include_bytes!("../../../../assets/skins/default/atlas.png"),
        )
    }

    pub fn skin_id(&self) -> &str {
        &self.skin_id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn tile_width(&self) -> u32 {
        self.tile_width
    }

    pub const fn tile_height(&self) -> u32 {
        self.tile_height
    }

    pub fn tile_rect(&self, tile: RenderTile) -> Result<AtlasRect, RenderError> {
        self.tiles
            .get(tile.atlas_key())
            .copied()
            .ok_or_else(|| RenderError::InvalidSkinAtlas {
                reason: format!("missing_atlas_tile:{}", tile.atlas_key()),
            })
    }

    pub fn paint_tile(
        &self,
        tile: RenderTile,
        output: &mut [u8],
        output_width: u32,
        destination_x: u32,
        destination_y: u32,
        cell_size: u32,
    ) -> Result<(), RenderError> {
        let rect = self.tile_rect(tile)?;
        for local_y in 0..cell_size {
            for local_x in 0..cell_size {
                let source_x = rect.x + local_x * rect.width / cell_size;
                let source_y = rect.y + local_y * rect.height / cell_size;
                let source = usize::try_from((source_y * self.width + source_x) * 4)
                    .expect("validated atlas dimensions fit usize");
                let destination = usize::try_from(
                    ((destination_y + local_y) * output_width + destination_x + local_x) * 4,
                )
                .expect("validated render dimensions fit usize");
                output[destination..destination + 4]
                    .copy_from_slice(&self.rgba[source..source + 4]);
            }
        }
        Ok(())
    }

    pub fn validate_png_header(bytes: &[u8]) -> Result<(), String> {
        const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
        if bytes.len() < PNG_SIGNATURE.len() {
            return Err("png_atlas_header_too_short".to_owned());
        }
        if &bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
            return Err("png_atlas_header_invalid".to_owned());
        }
        Ok(())
    }
}

fn collect_rects(
    manifest: &Value,
    field: &str,
    output: &mut BTreeMap<String, AtlasRect>,
) -> Result<(), RenderError> {
    let entries = manifest
        .get(field)
        .and_then(Value::as_object)
        .ok_or_else(|| RenderError::InvalidSkinManifest {
            reason: format!("missing_{field}"),
        })?;
    for (key, value) in entries {
        output.insert(
            key.clone(),
            AtlasRect {
                x: required_u32(value, "x")?,
                y: required_u32(value, "y")?,
                width: required_u32(value, "width")?,
                height: required_u32(value, "height")?,
            },
        );
    }
    Ok(())
}

fn decode_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), RenderError> {
    SelfContainedPngDecoder::decode(bytes)
}

struct SelfContainedPngDecoder;

impl SelfContainedPngDecoder {
    fn decode(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), RenderError> {
        SkinAtlas::validate_png_header(bytes)
            .map_err(|reason| RenderError::InvalidSkinAtlas { reason })?;
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder
            .read_info()
            .map_err(|error| RenderError::InvalidSkinAtlas {
                reason: error.to_string(),
            })?;
        let buffer_size =
            reader
                .output_buffer_size()
                .ok_or_else(|| RenderError::InvalidSkinAtlas {
                    reason: "png_output_buffer_size_overflow".to_owned(),
                })?;
        let mut decoded = vec![0; buffer_size];
        let info =
            reader
                .next_frame(&mut decoded)
                .map_err(|error| RenderError::InvalidSkinAtlas {
                    reason: error.to_string(),
                })?;
        let bytes = &decoded[..info.buffer_size()];
        let rgba = match info.color_type {
            png::ColorType::Rgba => bytes.to_vec(),
            png::ColorType::Rgb => bytes
                .chunks_exact(3)
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => bytes
                .chunks_exact(2)
                .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
                .collect(),
            png::ColorType::Grayscale => bytes
                .iter()
                .flat_map(|value| [*value, *value, *value, 255])
                .collect(),
            png::ColorType::Indexed => {
                return Err(RenderError::InvalidSkinAtlas {
                    reason: "indexed_png_not_expanded".to_owned(),
                });
            }
        };
        Ok((info.width, info.height, rgba))
    }
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, RenderError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| RenderError::InvalidSkinManifest {
            reason: format!("missing_{field}"),
        })
}

fn required_u32(value: &Value, field: &str) -> Result<u32, RenderError> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| RenderError::InvalidSkinManifest {
            reason: format!("missing_{field}"),
        })
}
