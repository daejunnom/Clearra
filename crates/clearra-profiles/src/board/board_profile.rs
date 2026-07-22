use clearra_core_domain::board::board_size::BoardSize;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoardProfileId {
    Standard10,
}

impl BoardProfileId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard10 => "standard-10",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardProfile {
    id: BoardProfileId,
    size: BoardSize,
}

impl BoardProfile {
    pub const fn new(id: BoardProfileId, size: BoardSize) -> Self {
        Self { id, size }
    }
}
impl BoardProfile {
    pub fn id(self) -> BoardProfileId {
        self.id
    }
}
impl BoardProfile {
    pub fn size(self) -> BoardSize {
        self.size
    }
}
impl BoardProfile {
    pub fn is_standard_10(self) -> bool {
        self.id == BoardProfileId::Standard10 && self.size.width() == 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_profile_ids_expose_stable_canonical_strings() {
        assert_eq!(BoardProfileId::Standard10.as_str(), "standard-10");
    }
}
