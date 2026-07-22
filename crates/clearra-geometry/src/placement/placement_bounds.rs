use clearra_piece_registry::registry::piece_registry::PieceRotationShape;

use crate::layout::board64_layout::Board64Layout;

pub fn shape_fits(layout: Board64Layout, shape: PieceRotationShape, x: u16, y: u16) -> bool {
    let origin_x = i32::from(x);
    let origin_y = i32::from(y);
    let width = i32::from(layout.width());
    let height = i32::from(layout.height());

    shape.cells().iter().all(|cell| {
        let absolute_x = origin_x + i32::from(cell.x());
        let absolute_y = origin_y + i32::from(cell.y());

        absolute_x >= 0 && absolute_y >= 0 && absolute_x < width && absolute_y < height
    })
}
