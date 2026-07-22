use super::custom_piece_definition::CustomPieceDefinition;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsupportedCustomPiece {
    definition: CustomPieceDefinition,
    reason: &'static str,
}

impl UnsupportedCustomPiece {
    pub fn new(definition: CustomPieceDefinition) -> Self {
        Self {
            definition,
            reason: "custom pieces have an MVP3 schema but are not connected to search runtime",
        }
    }
}
impl UnsupportedCustomPiece {
    pub fn definition(&self) -> &CustomPieceDefinition {
        &self.definition
    }
}
impl UnsupportedCustomPiece {
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}
