use serde_json::{Map, Value};

pub(super) fn required_object<'a>(
    value: &'a Value,
    pointer: &str,
) -> Result<&'a Map<String, Value>, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("external PC fixture missing {pointer}"))
}

pub(super) fn required_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("external PC fixture missing {pointer}"))
}

pub(super) fn optional_string<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

pub(super) fn scalar_string(input: &Map<String, Value>, key: &str) -> Option<String> {
    optional_string(input, key).map(ToOwned::to_owned)
}

pub(super) fn scalar_bool(input: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    input
        .get(key)
        .map(|value| parse_value_bool(value, key))
        .transpose()
}

pub(super) fn scalar_usize(input: &Map<String, Value>, key: &str) -> Result<Option<usize>, String> {
    input
        .get(key)
        .map(|value| parse_value_usize(value, key))
        .transpose()
}

pub(super) fn format_mask(mask: u64) -> String {
    format!("0x{mask:016x}")
}

fn parse_value_bool(value: &Value, key: &str) -> Result<bool, String> {
    match value {
        Value::Bool(value) => Ok(*value),
        Value::String(value) => parse_str_bool(value, key),
        _ => Err(format!("external PC fixture input.{key} must be a boolean")),
    }
}

fn parse_str_bool(value: &str, key: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(format!(
            "external PC fixture input.{key} boolean value '{value}' is invalid"
        )),
    }
}

fn parse_value_usize(value: &Value, key: &str) -> Result<usize, String> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| format!("external PC fixture input.{key} must fit usize")),
        Value::String(value) => parse_str_usize(value, key),
        _ => Err(format!(
            "external PC fixture input.{key} must be an integer"
        )),
    }
}

fn parse_str_usize(value: &str, key: &str) -> Result<usize, String> {
    value
        .trim()
        .parse()
        .map_err(|_| format!("external PC fixture input.{key} integer value '{value}' is invalid"))
}
