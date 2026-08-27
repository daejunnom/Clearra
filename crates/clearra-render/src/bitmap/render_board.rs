use crate::{
    export::{render_allocation_authority::RenderAllocationAuthority, RenderExportLimits},
    RenderError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderCell {
    Empty,
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
    Garbage,
}

impl RenderCell {
    pub fn from_char(value: char) -> Result<Self, RenderError> {
        match value.to_ascii_uppercase() {
            '.' | '_' | ' ' => Ok(Self::Empty),
            'I' => Ok(Self::I),
            'O' => Ok(Self::O),
            'T' => Ok(Self::T),
            'S' => Ok(Self::S),
            'Z' => Ok(Self::Z),
            'J' => Ok(Self::J),
            'L' => Ok(Self::L),
            'G' | 'X' => Ok(Self::Garbage),
            unknown => Err(RenderError::UnknownCell { value: unknown }),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderBoard {
    width: usize,
    height: usize,
    cells: Vec<RenderCell>,
}

impl RenderBoard {
    pub fn from_rows(rows: &[&str]) -> Result<Self, RenderError> {
        Self::from_rows_with_limits(rows, RenderExportLimits::product_default())
    }

    /// Constructs one bounded top-down render board from already typed cells.
    ///
    /// Codec adapters use this seam so document colors never take a lossy
    /// character/string round trip. The destination allocation remains under
    /// the renderer's product materialization authority.
    pub fn from_cells(
        width: usize,
        height: usize,
        cells_top_down: &[RenderCell],
    ) -> Result<Self, RenderError> {
        let limits = RenderExportLimits::product_default();
        let cell_capacity = limits.board_cell_capacity::<RenderCell>(width, height)?;
        if cells_top_down.len() != cell_capacity {
            return Err(RenderError::InvalidBoardRows);
        }
        let mut authority = RenderAllocationAuthority::new(limits.max_materialization_bytes());
        let mut cells =
            authority.try_vec_with_capacity::<RenderCell>(cell_capacity, "render_board_cells")?;
        cells.extend_from_slice(cells_top_down);
        Ok(Self {
            width,
            height,
            cells,
        })
    }

    fn from_rows_with_limits(
        rows: &[&str],
        limits: RenderExportLimits,
    ) -> Result<Self, RenderError> {
        let first = rows.first().ok_or(RenderError::InvalidBoardRows)?;
        let width = first.chars().count();
        if width == 0 || rows.iter().any(|row| row.chars().count() != width) {
            return Err(RenderError::InvalidBoardRows);
        }

        let cell_capacity = limits.board_cell_capacity::<RenderCell>(width, rows.len())?;
        for value in rows.iter().flat_map(|row| row.chars()) {
            RenderCell::from_char(value)?;
        }

        let mut authority = RenderAllocationAuthority::new(limits.max_materialization_bytes());
        let mut cells =
            authority.try_vec_with_capacity::<RenderCell>(cell_capacity, "render_board_cells")?;
        for value in rows.iter().flat_map(|row| row.chars()) {
            cells.push(RenderCell::from_char(value)?);
        }

        Ok(Self {
            width,
            height: rows.len(),
            cells,
        })
    }
}

#[cfg(test)]
mod tests {
    use core::mem::size_of;

    use super::{RenderBoard, RenderCell};
    use crate::{
        export::{
            render_allocation_authority::{allocation_attempts, reset_allocation_attempts},
            RenderExportLimits,
        },
        RenderError,
    };

    #[test]
    fn oversized_board_dimension_is_rejected_before_cell_allocation() {
        let row = ".".repeat(
            usize::try_from(RenderExportLimits::product_default().max_frame_width())
                .expect("product width")
                + 1,
        );
        reset_allocation_attempts();

        let error = RenderBoard::from_rows(&[row.as_str()]);

        assert_eq!(
            error,
            Err(RenderError::ExportLimitExceeded {
                limit: "max_frame_width",
                actual: 1921,
                max: 1920,
            })
        );
        assert_eq!(allocation_attempts(), 0);
    }

    #[test]
    fn oversized_board_pixel_count_is_rejected_before_cell_allocation() {
        let limits = RenderExportLimits::tight_for_tests().with_max_frame_pixels_for_test(7);
        reset_allocation_attempts();

        let error = RenderBoard::from_rows_with_limits(&["....", "...."], limits);

        assert_eq!(
            error,
            Err(RenderError::ExportLimitExceeded {
                limit: "max_frame_pixels",
                actual: 8,
                max: 7,
            })
        );
        assert_eq!(allocation_attempts(), 0);
    }

    #[test]
    fn oversized_board_memory_is_rejected_before_cell_allocation() {
        let required_bytes = 4 * size_of::<RenderCell>() as u64;
        let limits = RenderExportLimits::tight_for_tests()
            .with_max_materialization_bytes_for_test(required_bytes - 1);
        reset_allocation_attempts();

        let error = RenderBoard::from_rows_with_limits(&["..", ".."], limits);

        assert_eq!(
            error,
            Err(RenderError::ExportLimitExceeded {
                limit: "max_materialization_bytes",
                actual: required_bytes,
                max: required_bytes - 1,
            })
        );
        assert_eq!(allocation_attempts(), 0);
    }
}
impl RenderBoard {
    pub const fn width(&self) -> usize {
        self.width
    }
}
impl RenderBoard {
    pub const fn height(&self) -> usize {
        self.height
    }
}
impl RenderBoard {
    pub fn cell(&self, x: usize, y: usize) -> RenderCell {
        self.cells[y * self.width + x]
    }
}
impl RenderBoard {
    pub fn occupied_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let mut min_x = self.width;
        let mut min_y = self.height;
        let mut max_x = 0usize;
        let mut max_y = 0usize;
        let mut found = false;

        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(x, y) == RenderCell::Empty {
                    continue;
                }
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                found = true;
            }
        }

        found.then_some((min_x, min_y, max_x, max_y))
    }
}
