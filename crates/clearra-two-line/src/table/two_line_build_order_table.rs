use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TwoLineBuildOrderTable {
    orders: Vec<Vec<PieceKind>>,
}

impl TwoLineBuildOrderTable {
    pub fn new(orders: Vec<Vec<PieceKind>>) -> Self {
        Self { orders }
    }
}
impl TwoLineBuildOrderTable {
    pub fn orders(&self) -> &[Vec<PieceKind>] {
        &self.orders
    }
}
