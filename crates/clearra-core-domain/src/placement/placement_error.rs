#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementError {
    OutOfBounds,
    Collision,
}
