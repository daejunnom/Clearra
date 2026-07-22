use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{SkinAtlas, SkinManifestValidator, SkinProvenanceValidator};

pub struct RuntimeAssetGate;

impl RuntimeAssetGate {
    pub fn renderer_consumes_only_png_atlas(
        manifest: &Value,
        provenance: &Value,
        atlas_bytes: &[u8],
    ) -> Result<(), String> {
        SkinManifestValidator::validate_manifest(manifest)?;
        let skin_id = required_str(manifest, "skin_id")?;
        let atlas_path = required_str(manifest, "atlas_path")?;
        if !atlas_path.ends_with(".png") {
            return Err("runtime_atlas_must_be_png".to_owned());
        }
        SkinProvenanceValidator::validate_builtin_provenance(
            provenance,
            skin_id,
            file_name(atlas_path),
        )?;
        let expected_hash = provenance
            .get("atlas_png_sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing_atlas_png_sha256".to_owned())?;
        let actual_hash = format!("{:x}", Sha256::digest(atlas_bytes));
        if expected_hash != actual_hash {
            return Err("atlas_png_sha256_mismatch".to_owned());
        }
        SkinAtlas::from_manifest_and_png(manifest, atlas_bytes)
            .map_err(|error| format!("invalid_runtime_skin:{error:?}"))?;
        Ok(())
    }
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing_{field}"))
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
