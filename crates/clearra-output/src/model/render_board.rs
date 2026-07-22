#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderBoard {
    width: u16,
    height: u16,
    occupied_mask: u64,
}

impl RenderBoard {
    pub fn new(width: u16, height: u16, occupied_mask: u64) -> Self {
        Self {
            width,
            height,
            occupied_mask,
        }
    }
}
impl RenderBoard {
    pub fn width(self) -> u16 {
        self.width
    }
}
impl RenderBoard {
    pub fn height(self) -> u16 {
        self.height
    }
}
impl RenderBoard {
    pub fn occupied_mask(self) -> u64 {
        self.occupied_mask
    }
}
