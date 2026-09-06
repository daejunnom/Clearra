#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StandardTetrominoAreaRule;

impl StandardTetrominoAreaRule {
    pub const PIECE_AREA: usize = 4;
}
impl StandardTetrominoAreaRule {
    pub fn piece_areas(piece_count: usize) -> Vec<usize> {
        std::iter::repeat_n(Self::PIECE_AREA, piece_count).collect()
    }
}
impl StandardTetrominoAreaRule {
    pub fn can_fill_component_area(component_area: usize) -> bool {
        component_area.is_multiple_of(Self::PIECE_AREA)
    }
}

pub fn standard_area4_fast_path_unchanged() -> bool {
    StandardTetrominoAreaRule::PIECE_AREA == 4
        && StandardTetrominoAreaRule::piece_areas(3) == [4, 4, 4]
        && StandardTetrominoAreaRule::can_fill_component_area(8)
        && !StandardTetrominoAreaRule::can_fill_component_area(10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_area4_fast_path_unchanged_marker() {
        assert!(standard_area4_fast_path_unchanged());
    }
}
