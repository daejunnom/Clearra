#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ComboState {
    combo: u16,
}

impl ComboState {
    pub fn new(combo: u16) -> Self {
        Self { combo }
    }
}
impl ComboState {
    pub fn combo(self) -> u16 {
        self.combo
    }
}
impl ComboState {
    pub fn advance(self, cleared_lines: u8) -> Self {
        if cleared_lines == 0 {
            Self::default()
        } else {
            Self {
                combo: self.combo.saturating_add(1),
            }
        }
    }
}
