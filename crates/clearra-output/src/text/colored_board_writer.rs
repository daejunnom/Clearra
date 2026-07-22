use crate::model::render_board::RenderBoard;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ColoredBoardWriter;

impl ColoredBoardWriter {
    pub fn render_ascii(board: RenderBoard) -> String {
        let mut rows = Vec::new();
        for y in (0..board.height()).rev() {
            let mut row = String::new();
            for x in 0..board.width() {
                let index = u32::from(y) * u32::from(board.width()) + u32::from(x);
                let occupied = (board.occupied_mask() & (1_u64 << index)) != 0;
                row.push(if occupied { '#' } else { '.' });
            }
            rows.push(row);
        }
        rows.join("\n")
    }
}
