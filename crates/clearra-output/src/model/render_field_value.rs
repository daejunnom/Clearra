use crate::json::json_contract::JsonValue;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderFieldValue {
    String(String),
    Bool(bool),
    Number(String),
    Array(Vec<RenderFieldValue>),
    Object(Vec<RenderField>),
    Null,
}

impl RenderFieldValue {
    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }
}
impl RenderFieldValue {
    pub fn bool(value: bool) -> Self {
        Self::Bool(value)
    }
}
impl RenderFieldValue {
    pub fn number(value: impl Into<String>) -> Self {
        Self::Number(value.into())
    }
}
impl RenderFieldValue {
    pub fn array<I>(values: I) -> Self
    where
        I: IntoIterator<Item = RenderFieldValue>,
    {
        Self::Array(values.into_iter().collect())
    }
}
impl RenderFieldValue {
    pub fn object<I, K>(fields: I) -> Self
    where
        I: IntoIterator<Item = (K, RenderFieldValue)>,
        K: Into<String>,
    {
        Self::Object(
            fields
                .into_iter()
                .map(|(key, value)| RenderField::new(key, value))
                .collect(),
        )
    }
}
impl RenderFieldValue {
    pub fn as_text(&self) -> String {
        match self {
            Self::String(value) | Self::Number(value) => value.clone(),
            Self::Bool(value) => value.to_string(),
            Self::Array(values) => values
                .iter()
                .map(Self::as_text)
                .collect::<Vec<_>>()
                .join(","),
            Self::Object(fields) => fields
                .iter()
                .map(|field| format!("{}={}", field.key(), field.value().as_text()))
                .collect::<Vec<_>>()
                .join(","),
            Self::Null => "null".to_owned(),
        }
    }
}
impl RenderFieldValue {
    pub fn to_json_value(&self) -> JsonValue {
        match self {
            Self::String(value) => JsonValue::string(value),
            Self::Bool(value) => JsonValue::Bool(*value),
            Self::Number(value) => JsonValue::number(value),
            Self::Array(values) => JsonValue::array(values.iter().map(Self::to_json_value)),
            Self::Object(fields) => JsonValue::object(
                fields
                    .iter()
                    .map(|field| (field.key().to_owned(), field.value().to_json_value())),
            ),
            Self::Null => JsonValue::Null,
        }
    }
}

impl From<String> for RenderFieldValue {
    fn from(value: String) -> Self {
        Self::string(value)
    }
}

impl From<&str> for RenderFieldValue {
    fn from(value: &str) -> Self {
        Self::string(value)
    }
}

impl From<bool> for RenderFieldValue {
    fn from(value: bool) -> Self {
        Self::bool(value)
    }
}

macro_rules! impl_number_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for RenderFieldValue {
                fn from(value: $ty) -> Self {
                    Self::number(value.to_string())
                }
            }
        )*
    };
}

impl_number_value!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderField {
    key: String,
    value: RenderFieldValue,
}

impl RenderField {
    pub fn new(key: impl Into<String>, value: impl Into<RenderFieldValue>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}
impl RenderField {
    pub fn key(&self) -> &str {
        &self.key
    }
}
impl RenderField {
    pub fn value(&self) -> &RenderFieldValue {
        &self.value
    }
}

#[cfg(test)]
#[path = "render_field_value_tests.rs"]
mod tests;
