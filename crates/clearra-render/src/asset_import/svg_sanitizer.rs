use roxmltree::{Document, Node, NodeType};

use super::{AssetImportLimits, SvgSecurityScanner};

pub fn sanitize_svg(svg: &str, limits: &AssetImportLimits) -> Result<String, String> {
    SvgSecurityScanner::validate_svg(svg, limits)?;
    let document = Document::parse(svg).map_err(|_| "svg_xml_parse_failed".to_owned())?;
    let mut output = String::new();
    serialize_element(document.root_element(), true, &mut output)?;
    Ok(output)
}

fn serialize_element(node: Node<'_, '_>, root: bool, output: &mut String) -> Result<(), String> {
    let name = node.tag_name().name();
    output.push('<');
    output.push_str(name);
    if root {
        output.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
    }

    let mut attributes = node
        .attributes()
        .filter(|attribute| attribute.name() != "xmlns")
        .map(|attribute| (attribute.name(), attribute.value()))
        .collect::<Vec<_>>();
    attributes.sort_unstable_by(|left, right| left.0.cmp(right.0));
    for (name, value) in attributes {
        output.push(' ');
        output.push_str(name);
        output.push_str("=\"");
        escape_attribute(value, output);
        output.push('"');
    }

    let children = node
        .children()
        .filter(|child| match child.node_type() {
            NodeType::Element => true,
            NodeType::Text => child.text().is_some_and(|text| !text.trim().is_empty()),
            _ => false,
        })
        .collect::<Vec<_>>();
    if children.is_empty() {
        output.push_str("/>");
        return Ok(());
    }

    output.push('>');
    for child in children {
        match child.node_type() {
            NodeType::Element => serialize_element(child, false, output)?,
            NodeType::Text => {
                return Err("svg_text_content_forbidden".to_owned());
            }
            _ => {}
        }
    }
    output.push_str("</");
    output.push_str(name);
    output.push('>');
    Ok(())
}

fn escape_attribute(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(character),
        }
    }
}
