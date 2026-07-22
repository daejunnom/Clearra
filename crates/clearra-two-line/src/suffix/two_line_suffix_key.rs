#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TwoLineSuffixKey {
    board_mask: u64,
    queue_index: usize,
}

impl TwoLineSuffixKey {
    pub fn new(board_mask: u64, queue_index: usize) -> Self {
        Self {
            board_mask,
            queue_index,
        }
    }
}
impl TwoLineSuffixKey {
    pub fn board_mask(self) -> u64 {
        self.board_mask
    }
}
impl TwoLineSuffixKey {
    pub fn queue_index(self) -> usize {
        self.queue_index
    }
}
