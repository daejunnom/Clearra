use serde_json::Value;

pub struct SkinProvenanceValidator;

impl SkinProvenanceValidator {
    pub fn validate_builtin_provenance(
        provenance: &Value,
        expected_skin_id: &str,
        expected_atlas_path: &str,
    ) -> Result<(), String> {
        require_u64(provenance, "schema_version", 1)?;
        require_string(provenance, "skin_id", expected_skin_id)?;
        require_non_empty_string(provenance, "asset_kind")?;
        require_non_empty_string(provenance, "source")?;
        require_bool(provenance, "raw_svg_runtime_rendering", false)?;
        require_string(
            provenance,
            "svg_import_policy",
            "sanitize-rasterize-at-build-time",
        )?;
        require_string(provenance, "atlas_path", expected_atlas_path)?;
        require_string(provenance, "atlas_format", "png")?;
        require_non_empty_string(provenance, "license")?;
        require_non_empty_string(provenance, "redistribution")?;
        require_sha256(provenance, "original_file_sha256")?;
        require_sha256(provenance, "sanitized_svg_sha256")?;
        require_sha256(provenance, "atlas_png_sha256")?;
        require_sha256(provenance, "manifest_sha256")?;
        require_non_empty_string(provenance, "import_tool_version")?;
        Ok(())
    }
}

fn require_sha256(value: &Value, field: &str) -> Result<(), String> {
    let hash = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing_{field}"))?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("invalid_{field}"));
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
