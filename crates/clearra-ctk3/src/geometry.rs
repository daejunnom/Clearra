use crate::{Ctk3Operation, Ctk3Piece, Ctk3Rotation};

pub(crate) fn operation_rotations(piece: Ctk3Piece) -> &'static [Ctk3Rotation] {
    use Ctk3Rotation::{Left, Reverse, Right, Spawn};
    match piece {
        Ctk3Piece::O => &[Spawn],
        Ctk3Piece::I | Ctk3Piece::S | Ctk3Piece::Z => &[Spawn, Right],
        Ctk3Piece::T | Ctk3Piece::J | Ctk3Piece::L => &[Spawn, Right, Reverse, Left],
    }
}

pub(crate) fn operation_cells(operation: Ctk3Operation) -> [(i64, i64); 4] {
    operation_offsets(operation.piece, operation.rotation)
        .map(|(x, y)| (i64::from(operation.x) + x, i64::from(operation.y) + y))
}

fn operation_offsets(piece: Ctk3Piece, rotation: Ctk3Rotation) -> [(i64, i64); 4] {
    let mut offsets = match piece {
        Ctk3Piece::I => [(-1, 0), (0, 0), (1, 0), (2, 0)],
        Ctk3Piece::O => [(0, 0), (1, 0), (0, 1), (1, 1)],
        Ctk3Piece::T => [(-1, 0), (0, 0), (1, 0), (0, 1)],
        Ctk3Piece::S => [(-1, 0), (0, 0), (0, 1), (1, 1)],
        Ctk3Piece::Z => [(-1, 1), (0, 1), (0, 0), (1, 0)],
        Ctk3Piece::J => [(-1, 1), (-1, 0), (0, 0), (1, 0)],
        Ctk3Piece::L => [(1, 1), (-1, 0), (0, 0), (1, 0)],
    };
    if piece == Ctk3Piece::O {
        return offsets;
    }
    let turns = match rotation {
        Ctk3Rotation::Spawn => 0,
        Ctk3Rotation::Right => 1,
        Ctk3Rotation::Reverse => 2,
        Ctk3Rotation::Left => 3,
    };
    for _ in 0..turns {
        for (x, y) in &mut offsets {
            (*x, *y) = (*y, -*x);
        }
    }
    offsets
}

pub(crate) fn canonicalize_operation(operation: Ctk3Operation) -> Option<Ctk3Operation> {
    let mut target_cells = operation_cells(operation);
    target_cells.sort_by_key(|(x, y)| (*y, *x));
    operation_from_cells(operation.piece, target_cells)
}

pub(crate) fn operation_from_cells(
    piece: Ctk3Piece,
    mut target_cells: [(i64, i64); 4],
) -> Option<Ctk3Operation> {
    target_cells.sort_by_key(|(x, y)| (*y, *x));
    for rotation in operation_rotations(piece) {
        let offsets = operation_offsets(piece, *rotation);
        for target_cell in target_cells {
            for offset in offsets {
                let candidate_x = target_cell.0 - offset.0;
                let candidate_y = target_cell.1 - offset.1;
                let Ok(x) = i32::try_from(candidate_x) else {
                    continue;
                };
                let Ok(y) = i32::try_from(candidate_y) else {
                    continue;
                };
                let candidate = Ctk3Operation {
                    piece,
                    rotation: *rotation,
                    x,
                    y,
                };
                let cells = operation_cells(candidate);
                if cells.iter().all(|cell| target_cells.contains(cell)) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}
