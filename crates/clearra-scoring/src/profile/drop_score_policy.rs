#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DropScorePolicy {
    #[default]
    Disabled,
    HardDrop2SoftDrop1,
}

impl DropScorePolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "disabled" | "none" => Some(Self::Disabled),
            "hard-drop-2-soft-drop-1" => Some(Self::HardDrop2SoftDrop1),
            _ => None,
        }
    }
}
impl DropScorePolicy {
    pub fn requires_drop_events(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}
impl DropScorePolicy {
    pub fn hard_drop_score(self, dropped_cells: u16) -> u64 {
        match self {
            Self::Disabled => 0,
            Self::HardDrop2SoftDrop1 => u64::from(dropped_cells).saturating_mul(2),
        }
    }
}
impl DropScorePolicy {
    pub fn soft_drop_score(self, dropped_cells: u16) -> u64 {
        match self {
            Self::Disabled => 0,
            Self::HardDrop2SoftDrop1 => u64::from(dropped_cells),
        }
    }
}
impl DropScorePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::HardDrop2SoftDrop1 => "hard-drop-2-soft-drop-1",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_drop_2_soft_drop_1_scores_drop_cells_without_level_multiplier() {
        let policy = DropScorePolicy::HardDrop2SoftDrop1;

        assert_eq!(policy.hard_drop_score(7), 14);
        assert_eq!(policy.soft_drop_score(7), 7);
        assert_eq!(policy.as_str(), "hard-drop-2-soft-drop-1");
    }
}
