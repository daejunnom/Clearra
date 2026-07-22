#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SpinTargetId(String);

impl SpinTargetId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl SpinTargetId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
