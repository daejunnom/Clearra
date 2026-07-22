#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TwoLineTilingTable {
    tiling_masks: Vec<u64>,
}

impl TwoLineTilingTable {
    pub fn new(tiling_masks: Vec<u64>) -> Self {
        Self { tiling_masks }
    }
}
impl TwoLineTilingTable {
    pub fn tiling_masks(&self) -> &[u64] {
        &self.tiling_masks
    }
}
