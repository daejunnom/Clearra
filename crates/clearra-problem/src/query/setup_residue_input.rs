use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupCycleResetBorrowPolicy {
    ForbidPostCyclePieceUse,
    AllowPostCyclePieceUse,
}

impl Default for SetupCycleResetBorrowPolicy {
    fn default() -> Self {
        Self::ForbidPostCyclePieceUse
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupResidueInput {
    pieces: Vec<PieceKind>,
}

impl SetupResidueInput {
    pub fn new(pieces: Vec<PieceKind>) -> Self {
        Self { pieces }
    }

    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }

    pub fn remaining_count(&self) -> usize {
        self.pieces.len()
    }

    pub fn cycle(&self) -> Option<u8> {
        cycle_for_remaining_count(self.remaining_count())
    }

    pub fn duplicate_piece(&self) -> Option<PieceKind> {
        PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .find(|piece| self.pieces.iter().filter(|value| **value == *piece).count() == 2)
    }
}

impl Default for SetupResidueInput {
    fn default() -> Self {
        Self::new(PieceKind::STANDARD_TETROMINOES.to_vec())
    }
}

pub const fn cycle_for_remaining_count(remaining_count: usize) -> Option<u8> {
    match remaining_count {
        7 => Some(1),
        4 => Some(2),
        1 => Some(3),
        5 => Some(4),
        2 => Some(5),
        6 => Some(6),
        3 => Some(7),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_piece_count_determines_pc_cycle() {
        assert_eq!(cycle_for_remaining_count(7), Some(1));
        assert_eq!(cycle_for_remaining_count(4), Some(2));
        assert_eq!(cycle_for_remaining_count(1), Some(3));
        assert_eq!(cycle_for_remaining_count(5), Some(4));
        assert_eq!(cycle_for_remaining_count(2), Some(5));
        assert_eq!(cycle_for_remaining_count(6), Some(6));
        assert_eq!(cycle_for_remaining_count(3), Some(7));
        assert_eq!(cycle_for_remaining_count(0), None);
    }

    #[test]
    fn one_duplicate_identifies_the_explicit_hold_piece() {
        let input =
            SetupResidueInput::new(vec![PieceKind::S, PieceKind::I, PieceKind::O, PieceKind::S]);

        assert_eq!(input.duplicate_piece(), Some(PieceKind::S));
        assert_eq!(input.cycle(), Some(2));
    }
}
