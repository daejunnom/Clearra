use clearra_render::{
    sanitize_svg, AssetImportLimits, AssetImportReportValidator, RuntimeAssetGate,
    SkinProvenanceValidator, SvgSecurityScanner,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn svg_forbidden_script_rejected() {
    let svg = r#"<svg viewBox="0 0 1 1"><script>alert(1)</script></svg>"#;

    assert_eq!(
        SvgSecurityScanner::validate_svg(svg, &AssetImportLimits::default()),
        Err("forbidden_svg_script".to_owned())
    );
}

#[test]
fn svg_external_resource_rejected() {
    let svg = r#"<svg viewBox="0 0 1 1"><use href="https://example.invalid/a.svg#x"/></svg>"#;

    assert_eq!(
        SvgSecurityScanner::validate_svg(svg, &AssetImportLimits::default()),
        Err("svg_external_resource_forbidden".to_owned())
    );
}

#[test]
fn svg_size_limit_rejected() {
    let limits = AssetImportLimits {
        max_svg_bytes: 8,
        ..AssetImportLimits::default()
    };

    assert_eq!(
        SvgSecurityScanner::validate_svg("<svg></svg>", &limits),
        Err("svg_size_limit_exceeded".to_owned())
    );
}

#[test]
fn svg_path_complexity_limit_rejected() {
    let limits = AssetImportLimits {
        max_path_commands: 2,
        ..AssetImportLimits::default()
    };
    let svg = r#"<svg viewBox="0 0 10 10"><path d="M0 0 L1 1 L2 2 Z"/></svg>"#;

    assert_eq!(
        SvgSecurityScanner::validate_svg(svg, &limits),
        Err("svg_path_complexity_limit_exceeded".to_owned())
    );
}

#[test]
fn asset_import_writes_provenance() {
    let report: Value = serde_json::from_str(include_str!(
        "../../../assets/skins/default/import-report.json"
    ))
    .expect("import report");
    let provenance: Value = serde_json::from_str(include_str!(
        "../../../assets/skins/default/provenance.json"
    ))
    .expect("default provenance");
    let source = include_bytes!("../../../tools/asset-import/default_skin_source.svg");
    let sanitized = sanitize_svg(
        std::str::from_utf8(source).expect("utf8 svg"),
        &AssetImportLimits::default(),
    )
    .expect("sanitized");
    let atlas = include_bytes!("../../../assets/skins/default/atlas.png");
    let manifest = include_bytes!("../../../assets/skins/default/skin.json");

    assert_eq!(
        AssetImportReportValidator::validate_import_report(&report),
        Ok(())
    );
    assert_eq!(
        SkinProvenanceValidator::validate_builtin_provenance(&provenance, "default", "atlas.png"),
        Ok(())
    );
    assert_eq!(report["original_file_sha256"], sha256(source));
    assert_eq!(report["sanitized_svg_sha256"], sha256(sanitized.as_bytes()));
    assert_eq!(report["atlas_png_sha256"], sha256(atlas));
    assert_eq!(report["manifest_sha256"], sha256(manifest));
    assert_eq!(provenance["atlas_png_sha256"], sha256(atlas));
}

#[test]
fn renderer_consumes_only_png_atlas() {
    let manifest: Value =
        serde_json::from_str(include_str!("../../../assets/skins/default/skin.json"))
            .expect("test skin manifest");
    let provenance: Value = serde_json::from_str(include_str!(
        "../../../assets/skins/default/provenance.json"
    ))
    .expect("default provenance");
    let atlas = include_bytes!("../../../assets/skins/default/atlas.png");

    assert_eq!(
        RuntimeAssetGate::renderer_consumes_only_png_atlas(&manifest, &provenance, atlas),
        Ok(())
    );

    let mut svg_manifest = manifest;
    svg_manifest["atlas_path"] = json!("raw.svg");
    assert_eq!(
        RuntimeAssetGate::renderer_consumes_only_png_atlas(&svg_manifest, &provenance, atlas),
        Err("runtime_atlas_must_be_png".to_owned())
    );
}
