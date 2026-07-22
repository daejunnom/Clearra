use clearra_render::{
    AtlasBoundsValidator, RenderCapabilityReport, RenderFrameFormat, SkinAtlas,
    SkinManifestValidator, SkinProvenanceValidator,
};
use serde_json::{json, Value};

fn parse_json(text: &str) -> Value {
    serde_json::from_str(text).expect("valid JSON fixture")
}

fn manifest() -> Value {
    parse_json(include_str!("../../../assets/skins/default/skin.json"))
}

fn provenance() -> Value {
    parse_json(include_str!(
        "../../../assets/skins/default/provenance.json"
    ))
}

#[test]
fn default_product_skin_manifest_is_valid() {
    assert_eq!(
        SkinManifestValidator::validate_manifest(&manifest()),
        Ok(())
    );
    let atlas = SkinAtlas::builtin_default().expect("default atlas");
    assert_eq!((atlas.width(), atlas.height()), (144, 16));
}

#[test]
fn default_skin_provenance_is_valid() {
    assert_eq!(
        SkinProvenanceValidator::validate_builtin_provenance(&provenance(), "default", "atlas.png"),
        Ok(())
    );
}

#[test]
fn skin_manifest_requires_all_standard_pieces() {
    let mut invalid = manifest();
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
fn skin_manifest_rejects_out_of_bounds_atlas_rect() {
    let invalid = json!({
        "pieces": { "I": { "x": 144, "y": 0, "width": 16, "height": 16 } }
    });
    assert_eq!(
        AtlasBoundsValidator::validate_piece_rects(&invalid, 144, 16),
        Err("piece_rect_out_of_bounds:I".to_owned())
    );
}

#[test]
fn skin_provenance_required_for_builtin_asset() {
    let mut invalid = provenance();
    invalid
        .as_object_mut()
        .expect("provenance")
        .remove("atlas_png_sha256");
    assert_eq!(
        SkinProvenanceValidator::validate_builtin_provenance(&invalid, "default", "atlas.png"),
        Err("missing_atlas_png_sha256".to_owned())
    );
}

#[test]
fn renderer_capability_matches_exact_golden() {
    let golden = parse_json(include_str!(
        "../../../tests/golden/render/render_capability_exact.json"
    ));
    let png = RenderCapabilityReport::current()
        .capability_for(RenderFrameFormat::Png)
        .expect("png capability");
    assert_eq!(png.supported(), golden["supported"].as_bool().unwrap());
    assert_eq!(
        png.render_exact(),
        golden["render_exact"].as_bool().unwrap()
    );
    assert_eq!(png.unsupported_reason(), None);
}

#[test]
fn renderer_reports_exact_for_png_and_gif() {
    let report = RenderCapabilityReport::current();
    for format in [RenderFrameFormat::Png, RenderFrameFormat::Gif] {
        let capability = report.capability_for(format).expect("format capability");
        assert!(capability.supported());
        assert!(capability.render_exact());
        assert_eq!(capability.unsupported_reason(), None);
    }
}
