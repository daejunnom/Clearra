#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AllSpinScoreMapping {
    #[default]
    Disabled,
    NativeAllSpinTable,
    UseTSpinMiniTable,
}

impl AllSpinScoreMapping {
    pub fn requires_all_piece_classifier(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}
impl AllSpinScoreMapping {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NativeAllSpinTable => "native-all-spin-table",
            Self::UseTSpinMiniTable => "use-t-spin-mini-table",
        }
    }
}
