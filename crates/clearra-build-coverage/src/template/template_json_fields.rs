use serde_json::{Map, Value};

use super::template_json_error::TemplateJsonError;

pub(crate) fn object<'a>(
    value: &'a Value,
    context: &'static str,
) -> Result<&'a Map<String, Value>, TemplateJsonError> {
    value
        .as_object()
        .ok_or(TemplateJsonError::ExpectedObject { context })
}

pub(crate) fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: &'static str,
) -> Result<(), TemplateJsonError> {
    for field in object.keys() {
        if !allowed.contains(&field.as_str()) {
            return Err(TemplateJsonError::UnknownField {
                context,
                field: field.clone(),
            });
        }
    }

    Ok(())
}

pub(crate) fn required_field<'a>(
    object: &'a Map<String, Value>,
    context: &'static str,
    field: &'static str,
) -> Result<&'a Value, TemplateJsonError> {
    object
        .get(field)
        .ok_or(TemplateJsonError::MissingField { context, field })
}

pub(crate) fn optional_field<'a>(
    object: &'a Map<String, Value>,
    field: &'static str,
) -> Option<&'a Value> {
    object.get(field).filter(|value| !value.is_null())
}

pub(crate) fn required_array<'a>(
    object: &'a Map<String, Value>,
    context: &'static str,
    field: &'static str,
) -> Result<&'a [Value], TemplateJsonError> {
    required_field(object, context, field)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| invalid_field(context, field, "expected array"))
}

pub(crate) fn required_string(
    object: &Map<String, Value>,
    context: &'static str,
    field: &'static str,
) -> Result<String, TemplateJsonError> {
    required_field(object, context, field)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| invalid_field(context, field, "expected string"))
}

pub(crate) fn optional_string(
    object: &Map<String, Value>,
    context: &'static str,
    field: &'static str,
) -> Result<Option<String>, TemplateJsonError> {
    match optional_field(object, field) {
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| invalid_field(context, field, "expected string or null")),
        None => Ok(None),
    }
}

pub(crate) fn required_u64(
    object: &Map<String, Value>,
    context: &'static str,
    field: &'static str,
) -> Result<u64, TemplateJsonError> {
    required_field(object, context, field)?
        .as_u64()
        .ok_or_else(|| invalid_field(context, field, "expected unsigned integer"))
}

pub(crate) fn required_u32(
    object: &Map<String, Value>,
    context: &'static str,
    field: &'static str,
) -> Result<u32, TemplateJsonError> {
    let value = required_u64(object, context, field)?;
    u32::try_from(value).map_err(|_| invalid_field(context, field, "value exceeds u32"))
}

pub(crate) fn required_u16(
    object: &Map<String, Value>,
    context: &'static str,
    field: &'static str,
) -> Result<u16, TemplateJsonError> {
    let value = required_u64(object, context, field)?;
    u16::try_from(value).map_err(|_| invalid_field(context, field, "value exceeds u16"))
}

pub(crate) fn invalid_field(
    context: &'static str,
    field: &'static str,
    reason: impl Into<String>,
) -> TemplateJsonError {
    TemplateJsonError::InvalidField {
        context,
        field,
        reason: reason.into(),
    }
}
