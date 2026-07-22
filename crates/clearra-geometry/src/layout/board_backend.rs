use clearra_core_domain::board::board_size::BoardSize;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoardBackendKind {
    Board64,
    Board128,
    Board256,
    Wide,
}

pub trait BoardLayoutBackend: Copy + Eq {
    fn size(self) -> BoardSize;
    fn backend_kind(self) -> BoardBackendKind;

    fn width(self) -> u16 {
        self.size().width()
    }

    fn height(self) -> u16 {
        self.size().height()
    }

    fn cell_count(self) -> u32 {
        self.size().area()
    }
}

pub fn backend_kind_for_size(size: BoardSize) -> BoardBackendKind {
    match size.area() {
        0..=64 => BoardBackendKind::Board64,
        65..=128 => BoardBackendKind::Board128,
        129..=256 => BoardBackendKind::Board256,
        _ => BoardBackendKind::Wide,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooses_fast_path_backend_by_cell_count() {
        assert_eq!(
            backend_kind_for_size(BoardSize::new(10, 6).expect("size")),
            BoardBackendKind::Board64
        );
        assert_eq!(
            backend_kind_for_size(BoardSize::new(10, 12).expect("size")),
            BoardBackendKind::Board128
        );
        assert_eq!(
            backend_kind_for_size(BoardSize::standard_10x20()),
            BoardBackendKind::Board256
        );
    }
}
