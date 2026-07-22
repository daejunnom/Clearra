use clearra_output::model::{RenderField, RenderFieldValue};

pub fn string_field(key: impl Into<String>, value: impl Into<String>) -> RenderField {
    RenderField::new(key, RenderFieldValue::string(value))
}

pub fn bool_field(key: impl Into<String>, value: bool) -> RenderField {
    RenderField::new(key, RenderFieldValue::bool(value))
}

pub fn number_field(key: impl Into<String>, value: impl ToString) -> RenderField {
    RenderField::new(key, RenderFieldValue::number(value.to_string()))
}

pub fn string_array_field<I, V>(key: impl Into<String>, values: I) -> RenderField
where
    I: IntoIterator<Item = V>,
    V: Into<String>,
{
    RenderField::new(
        key,
        RenderFieldValue::array(values.into_iter().map(RenderFieldValue::string)),
    )
}

pub fn text_pairs(fields: &[RenderField]) -> Vec<(String, String)> {
    fields
        .iter()
        .map(|field| (field.key().to_owned(), field.value().as_text()))
        .collect()
}
