#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SetupSearchMode {
    #[default]
    ShapeOracle,
    QueueBased,
}

impl SetupSearchMode {
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::ShapeOracle => "oracle",
            Self::QueueBased => "qb",
        }
    }

    pub fn from_keyword(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "oracle" | "shape-oracle" => Some(Self::ShapeOracle),
            "qb" | "queue-based" => Some(Self::QueueBased),
            _ => None,
        }
    }
}
