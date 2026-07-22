use clearra_geometry::layout::board64_layout::Board64Layout;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Board64State {
    layout: Board64Layout,
    occupied: u64,
}

impl Board64State {
    pub fn empty(layout: Board64Layout) -> Self {
        Self {
            layout,
            occupied: 0,
        }
    }
}
impl Board64State {
    pub fn new(layout: Board64Layout, occupied: u64) -> Result<Self, Board64StateError> {
        let layout_mask = layout.all_cells_mask();
        if occupied & !layout_mask != 0 {
            return Err(Board64StateError::OccupancyOutsideLayout {
                occupied,
                layout_mask,
            });
        }
        Ok(Self { layout, occupied })
    }
}
impl Board64State {
    pub fn layout(self) -> Board64Layout {
        self.layout
    }
}
impl Board64State {
    pub fn occupied(self) -> u64 {
        self.occupied
    }
}
impl Board64State {
    pub fn is_empty(self) -> bool {
        self.occupied == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Board64StateError {
    OccupancyOutsideLayout { occupied: u64, layout_mask: u64 },
}
