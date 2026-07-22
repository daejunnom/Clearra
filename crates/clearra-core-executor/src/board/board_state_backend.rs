use clearra_core_domain::board::board_size::BoardSize;
use clearra_geometry::layout::board_backend::BoardBackendKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardBackendError {
    Collision,
    MaskOutsideLayout,
}

pub trait BoardStateBackend: Clone + Eq {
    type Mask: Clone + Eq;

    fn backend_kind(&self) -> BoardBackendKind;
    fn size(&self) -> BoardSize;
    fn occupied_count(&self) -> u32;
    fn row_mask(&self, y: u16) -> Option<Self::Mask>;
    fn singleton_mask(&self, cell_index: u32) -> Option<Self::Mask>;
    fn collides_mask(&self, mask: &Self::Mask) -> bool;
    fn place_mask(&self, mask: &Self::Mask) -> Result<Self, BoardBackendError>;
    fn clear_full_rows(&self) -> (Self, u8);

    fn is_empty(&self) -> bool {
        self.occupied_count() == 0
    }
}
