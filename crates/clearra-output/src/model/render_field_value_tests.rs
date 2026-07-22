use super::*;

#[test]
fn render_field_value_keeps_string_ids_distinct_from_numbers() {
    let value = RenderFieldValue::string("001");

    assert_eq!(value.as_text(), "001");
    assert_eq!(value.to_json_value(), JsonValue::string("001"));
}

#[test]
fn render_field_value_carries_explicit_bool_number_array_and_object() {
    let value = RenderFieldValue::object([
        ("ok", RenderFieldValue::bool(true)),
        ("count", RenderFieldValue::number("3")),
        (
            "ids",
            RenderFieldValue::array([
                RenderFieldValue::string("001"),
                RenderFieldValue::string("002"),
            ]),
        ),
    ]);

    assert_eq!(
        value.to_json_value(),
        JsonValue::object([
            ("ok", JsonValue::Bool(true)),
            ("count", JsonValue::number("3")),
            (
                "ids",
                JsonValue::array([JsonValue::string("001"), JsonValue::string("002")])
            )
        ])
    );
}
