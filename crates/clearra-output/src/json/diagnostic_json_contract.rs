use crate::json::{
    json_contract_helpers::{field_value, number_or_null, push_existing, string_or_null},
    json_value::{JsonField, JsonValue},
};

pub(crate) fn diagnostics_contract(fields: &[JsonField]) -> JsonValue {
    let mut members = Vec::new();
    push_existing(fields, &mut members, "diagnostic_count", "count");
    push_existing(fields, &mut members, "diagnostic_code", "code");
    push_existing(fields, &mut members, "diagnostic_severity", "severity");
    push_existing(fields, &mut members, "diagnostic_message", "message");
    push_existing(fields, &mut members, "diagnostic_location", "location");
    push_existing(
        fields,
        &mut members,
        "suggested_next_step",
        "suggested_next_step",
    );
    push_existing(fields, &mut members, "diagnostic_evidence", "evidence");
    if let Some(value) = field_value(fields, "diagnostics") {
        members.push(("items".to_owned(), value));
    } else if field_value(fields, "diagnostic_code").is_some() {
        members.push((
            "items".to_owned(),
            JsonValue::array([diagnostic_item(fields)]),
        ));
    } else {
        members.push(("items".to_owned(), JsonValue::array([])));
    }
    JsonValue::object(members)
}

fn diagnostic_item(fields: &[JsonField]) -> JsonValue {
    JsonValue::object([
        ("code", string_or_null(fields, "diagnostic_code")),
        ("severity", string_or_null(fields, "diagnostic_severity")),
        ("message", string_or_null(fields, "diagnostic_message")),
        ("location", string_or_null(fields, "diagnostic_location")),
        (
            "suggested_next_step",
            string_or_null(fields, "suggested_next_step"),
        ),
        (
            "evidence",
            field_value(fields, "diagnostic_evidence")
                .unwrap_or_else(|| JsonValue::object(Vec::<(String, JsonValue)>::new())),
        ),
        ("count", number_or_null(fields, "diagnostic_count")),
    ])
}
