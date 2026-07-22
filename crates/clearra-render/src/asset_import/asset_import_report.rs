use serde_json::Value;

pub struct AssetImportReportValidator;

impl AssetImportReportValidator {
    pub fn validate_import_report(report: &Value) -> Result<(), String> {
        require_u64(report, "schema_version", 1)?;
        require_string(report, "kind", "asset-import-security-report")?;
        require_non_empty_string(report, "source_label")?;
        require_origin_kind(report)?;
        require_sha256(report, "original_file_sha256")?;
        require_sha256(report, "sanitized_svg_sha256")?;
        require_sha256(report, "atlas_png_sha256")?;
        require_sha256(report, "manifest_sha256")?;
        require_non_empty_string(report, "license")?;
        require_non_empty_string(report, "redistribution")?;
        require_non_empty_string(report, "import_tool_version")?;
        require_non_empty_string(report, "security_report_id")?;
        validate_limit_snapshot(report)?;
        Ok(())
    }
}

fn require_origin_kind(report: &Value) -> Result<(), String> {
    let origin_kind = report
        .get("origin_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing_origin_kind".to_owned())?;
    match origin_kind {
        "external-svg" | "builtin-reviewed" | "human-reviewed-svg" | "test-fixture" => Ok(()),
        _ => Err("invalid_origin_kind".to_owned()),
    }
}

fn validate_limit_snapshot(report: &Value) -> Result<(), String> {
    let limits = report
        .get("limits")
        .ok_or_else(|| "missing_limits".to_owned())?;
    for field in [
        "max_svg_bytes",
        "max_decompressed_bytes",
        "max_elements",
        "max_group_depth",
        "max_path_commands",
        "max_path_segments_per_path",
        "max_gradients",
        "max_css_rules",
        "max_viewbox_width",
        "max_viewbox_height",
        "max_raster_pixels",
        "max_import_time_ms",
        "max_memory_mib",
    ] {
        require_positive_u64(limits, field)?;
    }
    require_u64(limits, "max_filters", 0)?;
    require_u64(limits, "max_external_references", 0)?;
    Ok(())
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

fn require_positive_u64(value: &Value, field: &str) -> Result<(), String> {
    match value.get(field).and_then(Value::as_u64) {
        Some(actual) if actual > 0 => Ok(()),
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
