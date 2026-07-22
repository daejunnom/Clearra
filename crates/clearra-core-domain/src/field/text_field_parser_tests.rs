use super::*;

#[test]
fn text_field_top_down_parses_to_bottom_up_mask() {
    let field = TextFieldParser::parse_top_down_rows(&["#.", ".#"]).expect("field");

    assert_eq!(field.width, 2);
    assert_eq!(field.height, 2);
    assert_eq!(field.mask, 0b0110);
}

#[test]
fn text_field_top_down_to_bottom_up_mask() {
    text_field_top_down_parses_to_bottom_up_mask();
}

#[test]
fn text_parser_rejects_invalid_cell() {
    let err = TextFieldParser::parse_top_down_rows(&["?."]).expect_err("invalid cell");

    assert_eq!(
        err,
        TextFieldParserError::InvalidCell {
            row: 0,
            column: 0,
            value: '?'
        }
    );
}
