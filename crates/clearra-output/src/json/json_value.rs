#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Object(Vec<JsonMember>),
    Array(Vec<JsonValue>),
}

impl JsonValue {
    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }
}
impl JsonValue {
    pub fn number(value: impl Into<String>) -> Self {
        Self::Number(value.into())
    }
}
impl JsonValue {
    pub fn object<I, K>(members: I) -> Self
    where
        I: IntoIterator<Item = (K, JsonValue)>,
        K: Into<String>,
    {
        Self::Object(
            members
                .into_iter()
                .map(|(key, value)| JsonMember::new(key, value))
                .collect(),
        )
    }
}
impl JsonValue {
    pub fn array<I>(values: I) -> Self
    where
        I: IntoIterator<Item = JsonValue>,
    {
        Self::Array(values.into_iter().collect())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonMember {
    key: String,
    value: JsonValue,
}

impl JsonMember {
    pub fn new(key: impl Into<String>, value: JsonValue) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }
}
impl JsonMember {
    pub fn key(&self) -> &str {
        &self.key
    }
}
impl JsonMember {
    pub fn value(&self) -> &JsonValue {
        &self.value
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonField {
    key: String,
    value: JsonValue,
}

impl JsonField {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: JsonValue::string(value),
        }
    }
}
impl JsonField {
    pub fn typed(key: impl Into<String>, value: JsonValue) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }
}
impl JsonField {
    pub fn key(&self) -> &str {
        &self.key
    }
}
impl JsonField {
    pub fn value(&self) -> &JsonValue {
        &self.value
    }
}
