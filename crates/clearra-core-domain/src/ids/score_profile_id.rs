#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScoreProfileId(String);

impl ScoreProfileId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}
impl ScoreProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
