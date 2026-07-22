use super::spin_accuracy::SpinAccuracy;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpinKind {
    #[default]
    None,
    RegularSpin,
    MiniSpin,
    TSpin,
    TSpinMini,
    AllSpin,
    AllSpinMini,
    ProfileSpecific(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinResult {
    piece: char,
    spin_kind: SpinKind,
    mini: bool,
    cleared_lines: u8,
    kick_used: bool,
    accuracy: SpinAccuracy,
}

impl SpinResult {
    pub fn none(piece: char, cleared_lines: u8, accuracy: SpinAccuracy) -> Self {
        Self {
            piece: piece.to_ascii_uppercase(),
            spin_kind: SpinKind::None,
            mini: false,
            cleared_lines,
            kick_used: false,
            accuracy,
        }
    }
}
impl SpinResult {
    pub fn new(
        piece: char,
        spin_kind: SpinKind,
        mini: bool,
        cleared_lines: u8,
        kick_used: bool,
        accuracy: SpinAccuracy,
    ) -> Self {
        Self {
            piece: piece.to_ascii_uppercase(),
            spin_kind,
            mini,
            cleared_lines,
            kick_used,
            accuracy,
        }
    }
}
impl SpinResult {
    pub fn piece(self) -> char {
        self.piece
    }
}
impl SpinResult {
    pub fn spin_kind(self) -> SpinKind {
        self.spin_kind
    }
}
impl SpinResult {
    pub fn is_spin(self) -> bool {
        !matches!(self.spin_kind, SpinKind::None)
    }
}
impl SpinResult {
    pub fn is_mini(self) -> bool {
        self.mini
    }
}
impl SpinResult {
    pub fn cleared_lines(self) -> u8 {
        self.cleared_lines
    }
}
impl SpinResult {
    pub fn kick_used(self) -> bool {
        self.kick_used
    }
}
impl SpinResult {
    pub fn accuracy(self) -> SpinAccuracy {
        self.accuracy
    }
}
