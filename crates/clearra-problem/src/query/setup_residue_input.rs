use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SetupCycleResetBorrowPolicy {
    #[default]
    ForbidPostCyclePieceUse,
    AllowPostCyclePieceUse,
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
        let mut duplicate = None;
        for piece in PieceKind::STANDARD_TETROMINOES {
            match self.pieces.iter().filter(|value| **value == piece).count() {
                0 | 1 => {}
                2 if duplicate.is_none() => duplicate = Some(piece),
                _ => return None,
            }
        }
        duplicate
    }

    pub fn has_valid_piece_multiplicity(&self) -> bool {
        let mut duplicated_kinds = 0;
        for piece in PieceKind::STANDARD_TETROMINOES {
            match self.pieces.iter().filter(|value| **value == piece).count() {
                0 | 1 => {}
                2 => duplicated_kinds += 1,
                _ => return false,
            }
        }
        duplicated_kinds <= 1
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
    fn duplicate_piece_reports_the_only_inventory_kind_available_for_automatic_hold() {
        let input =
            SetupResidueInput::new(vec![PieceKind::S, PieceKind::I, PieceKind::O, PieceKind::S]);

        assert_eq!(input.duplicate_piece(), Some(PieceKind::S));
        assert!(input.has_valid_piece_multiplicity());
        assert_eq!(input.cycle(), Some(2));
    }

    #[test]
    fn duplicate_piece_rejects_two_repeated_kinds_or_three_copies() {
        for pieces in [
            vec![PieceKind::S, PieceKind::S, PieceKind::I, PieceKind::I],
            vec![PieceKind::S, PieceKind::S, PieceKind::S, PieceKind::I],
        ] {
            let input = SetupResidueInput::new(pieces);
            assert_eq!(input.duplicate_piece(), None);
            assert!(!input.has_valid_piece_multiplicity());
        }
    }
}
