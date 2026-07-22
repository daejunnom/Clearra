use clearra_core_domain::board::board_size::BoardSize;

use super::board_backend::{BoardBackendKind, BoardLayoutBackend};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WideBoardLayout {
    size: BoardSize,
}

impl WideBoardLayout {
    pub fn new(size: BoardSize) -> Self {
        Self { size }
    }
}
impl WideBoardLayout {
    pub fn size(self) -> BoardSize {
        self.size
    }
}
impl WideBoardLayout {
    pub fn width(self) -> u16 {
        self.size.width()
    }
}
impl WideBoardLayout {
    pub fn height(self) -> u16 {
        self.size.height()
    }
}
impl WideBoardLayout {
    pub fn cell_count(self) -> u32 {
        self.size.area()
    }
}

impl BoardLayoutBackend for WideBoardLayout {
    fn size(self) -> BoardSize {
        self.size()
    }

    fn backend_kind(self) -> BoardBackendKind {
        BoardBackendKind::Wide
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::board::board_size::BoardSize;

    use super::*;

    #[test]
    fn wide_layout_keeps_custom_size_without_bit_width_limit() {
        let layout = WideBoardLayout::new(BoardSize::new(16, 20).expect("custom board"));

        assert_eq!(layout.width(), 16);
        assert_eq!(layout.height(), 20);
        assert_eq!(layout.cell_count(), 320);
        assert_eq!(layout.backend_kind(), BoardBackendKind::Wide);
    }
}
