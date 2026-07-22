use super::*;

#[test]
fn accepts_cells_inside_board() {
    let size = BoardSize::new(10, 20).expect("valid board size");
    let coord = CellCoord::new(9, 19, size).expect("cell inside board");

    assert_eq!(coord.x(), 9);
    assert_eq!(coord.y(), 19);
}

#[test]
fn rejects_cells_outside_board() {
    let size = BoardSize::new(10, 20).expect("valid board size");

    assert_eq!(
        CellCoord::new(10, 0, size),
        Err(CellCoordError::XOutOfBounds { x: 10, width: 10 })
    );
    assert_eq!(
        CellCoord::new(0, 20, size),
        Err(CellCoordError::YOutOfBounds { y: 20, height: 20 })
    );
}
