use crate::json::json_value::{JsonField, JsonMember, JsonValue};
pub(crate) fn pick_object(fields: &[JsonField], keys: &[&str]) -> JsonValue {
    let mut members = Vec::new();
    for key in keys {
        push_existing(fields, &mut members, key, key);
    }
    JsonValue::object(members)
}

pub(crate) fn prefixed_object(fields: &[JsonField], prefix: &str) -> JsonValue {
    JsonValue::Object(
        fields
            .iter()
            .filter_map(|field| {
                field
                    .key()
                    .strip_prefix(prefix)
                    .map(|key| JsonMember::new(key, field.value().clone()))
            })
            .collect(),
    )
}

pub(crate) fn indexed_results(fields: &[JsonField], prefix: &str) -> JsonValue {
    let mut result_indexes = Vec::new();
    for field in fields {
        let Some(rest) = field.key().strip_prefix(prefix) else {
            continue;
        };
        let Some((index, _)) = rest.split_once('_') else {
            continue;
        };
        if !result_indexes.iter().any(|known| known == index) {
            result_indexes.push(index.to_owned());
        }
    }

    JsonValue::array(result_indexes.into_iter().map(|index| {
        let item_prefix = format!("{prefix}{index}_");
        prefixed_object(fields, &item_prefix)
    }))
}

pub(crate) fn push_existing(
    fields: &[JsonField],
    members: &mut Vec<(String, JsonValue)>,
    source_key: &str,
    target_key: &str,
) {
    if let Some(value) = field_value(fields, source_key) {
        members.push((target_key.to_owned(), value));
    }
}

pub(crate) fn field_value(fields: &[JsonField], key: &str) -> Option<JsonValue> {
    fields
        .iter()
        .find_map(|field| (field.key() == key).then(|| field.value().clone()))
}

pub(crate) fn string_or_null(fields: &[JsonField], key: &str) -> JsonValue {
    nullable_string_value(field_value(fields, key))
}

pub(crate) fn string_or_null_fallback(
    fields: &[JsonField],
    preferred: &str,
    fallback: &str,
) -> JsonValue {
    nullable_backend_string_value(
        field_value(fields, preferred).or_else(|| field_value(fields, fallback)),
    )
}

pub(crate) fn number_or_null(fields: &[JsonField], key: &str) -> JsonValue {
    nullable_number_value(field_value(fields, key))
}

pub(crate) fn bool_or_false(fields: &[JsonField], key: &str) -> JsonValue {
    match field_value(fields, key) {
        Some(JsonValue::Bool(value)) => JsonValue::Bool(value),
        Some(JsonValue::String(value)) if value == "true" => JsonValue::Bool(true),
        Some(JsonValue::String(value)) if value == "false" => JsonValue::Bool(false),
        _ => JsonValue::Bool(false),
    }
}

pub(crate) fn nullable_string_value(value: Option<JsonValue>) -> JsonValue {
    match value {
        Some(JsonValue::String(value)) if value == "none" || value == "auto" => JsonValue::Null,
        Some(JsonValue::String(value)) if value.is_empty() => JsonValue::Null,
        Some(value) => value,
        None => JsonValue::Null,
    }
}

pub(crate) fn nullable_backend_string_value(value: Option<JsonValue>) -> JsonValue {
    match value {
        Some(JsonValue::String(value)) if value == "none" || value.is_empty() => JsonValue::Null,
        Some(value) => value,
        None => JsonValue::Null,
    }
}

pub(crate) fn nullable_number_value(value: Option<JsonValue>) -> JsonValue {
    match value {
        Some(JsonValue::Number(value)) if value == "none" || value == "auto" => JsonValue::Null,
        Some(JsonValue::Number(value)) => JsonValue::Number(value),
        Some(JsonValue::String(value)) if value == "none" || value == "auto" => JsonValue::Null,
        Some(JsonValue::String(value)) if value.is_empty() => JsonValue::Null,
        Some(JsonValue::String(value)) if value.chars().all(|c| c.is_ascii_digit()) => {
            JsonValue::Number(value)
        }
        Some(value) => value,
        None => JsonValue::Null,
    }
}

pub(crate) fn nullable_device_label_value(value: Option<JsonValue>) -> JsonValue {
    match value {
        Some(JsonValue::String(value)) if value == "none" || value.is_empty() => JsonValue::Null,
        Some(value) => value,
        None => JsonValue::Null,
    }
}

pub(crate) fn string_value_is(value: &JsonValue, expected: &str) -> bool {
    matches!(value, JsonValue::String(actual) if actual == expected)
}

pub(crate) fn trace_key_values(value: &str) -> Vec<JsonValue> {
    if value.is_empty() || value == "none" {
        return Vec::new();
    }
    value.split(',').map(JsonValue::string).collect()
}
