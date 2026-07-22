#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionKind {
    Empty,
    Occupied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaComponent {
    kind: RegionKind,
    cells: Vec<u32>,
}

impl AreaComponent {
    pub fn new(kind: RegionKind, mut cells: Vec<u32>) -> Self {
        cells.sort_unstable();
        cells.dedup();
        Self { kind, cells }
    }
}
impl AreaComponent {
    pub fn kind(&self) -> RegionKind {
        self.kind
    }
}
impl AreaComponent {
    pub fn cells(&self) -> &[u32] {
        &self.cells
    }
}
impl AreaComponent {
    pub fn area(&self) -> usize {
        self.cells.len()
    }
}
impl AreaComponent {
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}
