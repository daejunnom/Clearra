#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScoreObjectiveCellId(String);

impl ScoreObjectiveCellId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl ScoreObjectiveCellId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
