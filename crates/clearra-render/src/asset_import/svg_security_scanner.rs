use roxmltree::{Document, Node};

use super::AssetImportLimits;

pub struct SvgSecurityScanner;

impl SvgSecurityScanner {
    pub fn validate_svg(svg: &str, limits: &AssetImportLimits) -> Result<(), String> {
        if svg.len() > limits.max_svg_bytes {
            return Err("svg_size_limit_exceeded".to_owned());
        }
        if svg.len() > limits.max_decompressed_bytes {
            return Err("svg_decompressed_size_limit_exceeded".to_owned());
        }
        reject_document_level_features(svg)?;
        let document = Document::parse(svg).map_err(|_| "svg_xml_parse_failed".to_owned())?;
        let elements = document
            .descendants()
            .filter(Node::is_element)
            .collect::<Vec<_>>();
        if elements.len() > limits.max_elements {
            return Err("svg_element_limit_exceeded".to_owned());
        }

        let mut gradients = 0_usize;
        let mut css_rules = 0_usize;
        let mut total_path_commands = 0_usize;
        for element in elements {
            validate_element(element, limits)?;
            let depth = element.ancestors().filter(Node::is_element).count();
            if depth > limits.max_group_depth {
                return Err("svg_group_depth_limit_exceeded".to_owned());
            }
            match element.tag_name().name() {
                "linearGradient" | "radialGradient" => gradients += 1,
                "style" => {
                    css_rules += element.text().unwrap_or_default().matches('{').count();
                }
                "path" => {
                    let commands = element.attribute("d").map(count_path_commands).unwrap_or(0);
                    if commands > limits.max_path_segments_per_path {
                        return Err("svg_path_segment_limit_exceeded".to_owned());
                    }
                    total_path_commands = total_path_commands.saturating_add(commands);
                }
                _ => {}
            }
        }
        if gradients > limits.max_gradients {
            return Err("svg_gradient_limit_exceeded".to_owned());
        }
        if css_rules > limits.max_css_rules {
            return Err("svg_css_rule_limit_exceeded".to_owned());
        }
        if total_path_commands > limits.max_path_commands {
            return Err("svg_path_complexity_limit_exceeded".to_owned());
        }
        validate_viewbox(&document, limits)?;
        validate_memory_budget(svg.len(), &document, limits)?;
        Ok(())
    }
}

fn reject_document_level_features(svg: &str) -> Result<(), String> {
    let lower = svg.to_ascii_lowercase();
    if lower.contains("<!doctype") || lower.contains("<!entity") {
        return Err("forbidden_svg_document_type".to_owned());
    }
    if lower.contains("<?") {
        return Err("forbidden_svg_processing_instruction".to_owned());
    }
    Ok(())
}

fn validate_element(element: Node<'_, '_>, limits: &AssetImportLimits) -> Result<(), String> {
    let name = element.tag_name().name();
    match name {
        "script" => return Err("forbidden_svg_script".to_owned()),
        "foreignObject" => return Err("forbidden_svg_foreign_object".to_owned()),
        "animate" | "animateMotion" | "animateTransform" | "set" => {
            return Err("forbidden_svg_animation".to_owned());
        }
        "filter" if limits.max_filters == 0 => return Err("forbidden_svg_filter".to_owned()),
        "image" | "use" | "a" => return Err("svg_external_resource_forbidden".to_owned()),
        "text" | "tspan" => return Err("forbidden_svg_remote_font".to_owned()),
        "style" => {
            let css = element.text().unwrap_or_default().to_ascii_lowercase();
            if css.contains("@import") || css.contains("url(") || css.contains("@font-face") {
                return Err("forbidden_svg_css_import".to_owned());
            }
            return Err("forbidden_svg_style_element".to_owned());
        }
        "svg" | "g" | "defs" | "linearGradient" | "radialGradient" | "stop" | "path" | "rect"
        | "circle" | "ellipse" | "line" | "polyline" | "polygon" => {}
        _ => return Err(format!("unsupported_svg_element:{name}")),
    }

    for attribute in element.attributes() {
        let attribute_name = attribute.name();
        let value = attribute.value();
        let lower_name = attribute_name.to_ascii_lowercase();
        let lower_value = value.to_ascii_lowercase();
        if lower_name.starts_with("on") {
            return Err("forbidden_svg_event_handler".to_owned());
        }
        if matches!(lower_name.as_str(), "href" | "src") {
            return Err("svg_external_resource_forbidden".to_owned());
        }
        if lower_name == "filter" {
            return Err("forbidden_svg_filter".to_owned());
        }
        if lower_value.contains("javascript:")
            || lower_value.contains("file:")
            || lower_value.contains("http:")
            || lower_value.contains("https:")
            || lower_value.contains("data:")
        {
            return Err("svg_external_resource_forbidden".to_owned());
        }
        if lower_value.contains("url(") && !is_internal_paint_reference(&lower_value) {
            return Err("svg_external_resource_forbidden".to_owned());
        }
        if !allowed_attribute(attribute_name) {
            return Err(format!("unsupported_svg_attribute:{attribute_name}"));
        }
    }
    Ok(())
}

fn allowed_attribute(name: &str) -> bool {
    matches!(
        name,
        "id" | "viewBox"
            | "width"
            | "height"
            | "preserveAspectRatio"
            | "transform"
            | "fill"
            | "fill-opacity"
            | "stroke"
            | "stroke-width"
            | "stroke-opacity"
            | "stroke-linecap"
            | "stroke-linejoin"
            | "opacity"
            | "d"
            | "x"
            | "y"
            | "rx"
            | "ry"
            | "cx"
            | "cy"
            | "r"
            | "x1"
            | "y1"
            | "x2"
            | "y2"
            | "points"
            | "offset"
            | "stop-color"
            | "stop-opacity"
            | "gradientUnits"
            | "gradientTransform"
            | "fx"
            | "fy"
            | "spreadMethod"
    )
}

fn is_internal_paint_reference(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("url(#") && value.ends_with(')')
}

fn count_path_commands(path_data: &str) -> usize {
    path_data
        .chars()
        .filter(|character| character.is_ascii_alphabetic())
        .count()
}

fn validate_viewbox(document: &Document<'_>, limits: &AssetImportLimits) -> Result<(), String> {
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return Err("svg_root_element_required".to_owned());
    }
    let Some(viewbox) = root.attribute("viewBox") else {
        return Ok(());
    };
    let parts = viewbox
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid_svg_viewbox".to_owned())?;
    if parts.len() != 4 || parts[2] <= 0.0 || parts[3] <= 0.0 {
        return Err("invalid_svg_viewbox".to_owned());
    }
    if parts[2] > f64::from(limits.max_viewbox_width)
        || parts[3] > f64::from(limits.max_viewbox_height)
    {
        return Err("svg_viewbox_limit_exceeded".to_owned());
    }
    if parts[2] * parts[3] > limits.max_raster_pixels as f64 {
        return Err("svg_raster_pixel_limit_exceeded".to_owned());
    }
    Ok(())
}

fn validate_memory_budget(
    source_bytes: usize,
    document: &Document<'_>,
    limits: &AssetImportLimits,
) -> Result<(), String> {
    let root = document.root_element();
    let width = parse_dimension(root.attribute("width")).unwrap_or(limits.max_viewbox_width as u64);
    let height =
        parse_dimension(root.attribute("height")).unwrap_or(limits.max_viewbox_height as u64);
    let estimated = width
        .saturating_mul(height)
        .saturating_mul(4)
        .saturating_add(source_bytes as u64);
    if estimated > limits.max_memory_mib.saturating_mul(1024 * 1024) {
        return Err("svg_memory_limit_exceeded".to_owned());
    }
    Ok(())
}

fn parse_dimension(value: Option<&str>) -> Option<u64> {
    value?
        .trim_end_matches("px")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.ceil() as u64)
}
