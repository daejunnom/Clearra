#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PieceId(u32);

impl PieceId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl PieceId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PieceDefinitionId(String);

impl PieceDefinitionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl PieceDefinitionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piece_definition_id_is_stable_string_identity_not_order_index() {
        let id = PieceDefinitionId::new("custom:tri-v1");

        assert_eq!(id.as_str(), "custom:tri-v1");
        assert!(PieceDefinitionId::new("custom:tri-v1") < PieceDefinitionId::new("std:I"));
    }
}
