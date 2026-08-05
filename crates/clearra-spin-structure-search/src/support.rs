use crate::board::StructureBoard;

/// Grounding and immobility are exact local predicates over compiled movement
/// neighbors.  An absent neighbor represents a wall or floor, not a heuristic.
pub(crate) fn grounded(
    y: i8,
    down: Option<usize>,
    masks: &[StructureBoard],
    board: StructureBoard,
) -> bool {
    y == 0 || down.is_none_or(|target| board.intersects(masks[target]))
}

pub(crate) fn immobile(
    neighbors: [Option<usize>; 4],
    masks: &[StructureBoard],
    board: StructureBoard,
) -> bool {
    neighbors
        .into_iter()
        .all(|target| target.is_none_or(|index| board.intersects(masks[index])))
}
