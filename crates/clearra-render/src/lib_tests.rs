use super::*;

fn default_manifest() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../assets/skins/default/skin.json"))
        .expect("default manifest")
}

fn default_provenance() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../assets/skins/default/provenance.json"
    ))
    .expect("default provenance")
}

#[test]
fn render_domain_types_load_connected_default_atlas() {
    let manifest = SkinManifest::new("default", "atlas.png", SkinProvenance::BuiltIn);
    let atlas = SkinAtlas::builtin_default().expect("validated atlas");
    let options = RenderOptions::new(160, 320, manifest.skin_id());

    assert_eq!(manifest.skin_id(), "default");
    assert_eq!(atlas.skin_id(), "default");
    assert_eq!(atlas.tile_width(), 16);
    assert_eq!(atlas.width(), 144);
    assert_eq!(options.skin_id(), "default");
}

#[test]
fn render_reports_png_and_gif_connected_exact() {
    let report = RenderCapabilityReport::current();
    for format in [RenderFrameFormat::Png, RenderFrameFormat::Gif] {
        let capability = report.capability_for(format).expect("capability");
        assert!(capability.supported());
        assert!(capability.render_exact());
        assert_eq!(capability.unsupported_reason(), None);
    }
}

#[test]
fn unsupported_frame_format_error_carries_capability_reason() {
    assert_eq!(
        RenderError::UnsupportedFrameFormat {
            frame_format: RenderFrameFormat::Png,
            reason: RenderUnsupportedReason::MissingValidatedSkin,
        },
        RenderError::UnsupportedFrameFormat {
            frame_format: RenderFrameFormat::Png,
            reason: RenderUnsupportedReason::MissingValidatedSkin,
        }
    );
}

#[test]
fn render_default_assets_include_manifest_provenance_and_png_atlas() {
    let manifest = include_str!("../../../assets/skins/default/skin.json");
    let provenance = include_str!("../../../assets/skins/default/provenance.json");
    let atlas = include_bytes!("../../../assets/skins/default/atlas.png");

    assert!(manifest.contains("\"render_exact\": true"));
    assert!(manifest.contains("\"runtime_raw_svg_allowed\": false"));
    assert!(provenance.contains("\"raw_svg_runtime_rendering\": false"));
    assert!(provenance.contains("sanitize-rasterize-at-build-time"));
    assert!(SkinAtlas::validate_png_header(atlas).is_ok());
}

#[test]
fn runtime_raw_svg_rejected() {
    let mut manifest = default_manifest();
    manifest["runtime_raw_svg_allowed"] = serde_json::json!(true);
    assert_eq!(
        SkinManifestValidator::validate_manifest(&manifest),
        Err("invalid_runtime_raw_svg_allowed".to_owned())
    );
}

#[test]
fn skin_manifest_requires_all_standard_pieces() {
    let mut invalid = default_manifest();
    invalid["pieces"]
        .as_object_mut()
        .expect("pieces")
        .remove("T");
    assert_eq!(
        SkinManifestValidator::validate_standard_piece_mapping(&invalid),
        Err("missing_piece:T".to_owned())
    );
}

#[test]
fn asset_provenance_required() {
    let provenance = default_provenance();
    assert!(SkinProvenanceValidator::validate_builtin_provenance(
        &provenance,
        "default",
        "atlas.png"
    )
    .is_ok());

    let mut missing = provenance;
    missing.as_object_mut().expect("object").remove("license");
    assert_eq!(
        SkinProvenanceValidator::validate_builtin_provenance(&missing, "default", "atlas.png"),
        Err("missing_license".to_owned())
    );
}

#[test]
fn png_atlas_header_validated() {
    let atlas = include_bytes!("../../../assets/skins/default/atlas.png");
    assert!(SkinAtlas::validate_png_header(atlas).is_ok());
    assert_eq!(
        SkinAtlas::validate_png_header(b"<svg></svg>"),
        Err("png_atlas_header_invalid".to_owned())
    );
}

#[test]
fn render_fixture_contracts_pin_exact_skin_manifest() {
    let manifest = include_str!("../../../assets/skins/default/skin.json");
    assert!(manifest.contains("\"required_pieces\""));
    assert!(manifest.contains("\"special\""));
    assert!(manifest.contains("\"render_exact\": true"));
    assert!(manifest.contains("\"supported\": true"));
    assert!(!manifest.contains("unsupported_reason"));
}

#[test]
fn render_capability_golden_reports_connected_exact() {
    let golden = include_str!("../../../tests/golden/render/render_capability_exact.json");
    let png = RenderCapabilityReport::current()
        .capability_for(RenderFrameFormat::Png)
        .expect("png capability");
    assert!(png.supported());
    assert!(png.render_exact());
    assert!(golden.contains("\"render_exact\": true"));
    assert!(golden.contains("\"supported\": true"));
}

#[test]
fn render_capability_reports_exact_for_both_formats() {
    for format in [RenderFrameFormat::Png, RenderFrameFormat::Gif] {
        let capability = RenderCapabilityReport::current()
            .capability_for(format)
            .expect("capability");
        assert!(capability.supported());
        assert!(capability.render_exact());
    }
}

#[test]
fn renderer_connected_exact() {
    render_capability_reports_exact_for_both_formats();
}

#[test]
fn exact_png_and_gif_requests_are_accepted() {
    for format in [RenderFrameFormat::Png, RenderFrameFormat::Gif] {
        let capability = RenderExactnessGate::request_exact_frame(format).expect("exact format");
        assert!(capability.supported());
        assert!(capability.render_exact());
    }
}
