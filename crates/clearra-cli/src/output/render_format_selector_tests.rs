use super::*;

#[test]
fn selects_supported_formats() {
    assert_eq!(RenderFormatSelector::parse(None), Ok(RenderFormat::Text));
    assert_eq!(
        RenderFormatSelector::parse(Some("json")),
        Ok(RenderFormat::Json)
    );
    assert_eq!(
        RenderFormatSelector::parse(Some("fumen-like")),
        Ok(RenderFormat::FumenLike)
    );
}

#[test]
fn rejects_png_and_gif_until_bitmap_render_capability_is_supported() {
    assert_eq!(
        RenderFormatSelector::parse(Some("png")),
        Err(RenderFormatSelectionError::UnsupportedFormat {
            value: "png".to_owned()
        })
    );
    assert_eq!(
        RenderFormatSelector::parse(Some("gif")),
        Err(RenderFormatSelectionError::UnsupportedFormat {
            value: "gif".to_owned()
        })
    );
}
